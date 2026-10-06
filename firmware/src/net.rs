//! Thread jaringan: WiFi, MQTT ThingsBoard (heartbeat, perintah RPC, data mentah
//! & hasil), OTA.
//!
//! Semua koneksi KELUAR dari ESP32 (MQTT/TLS ke ThingsBoard), jadi perangkat bisa
//! dikendalikan dari dashboard publik walau berada di balik NAT/WiFi kampus.

use crate::config::*;
use crate::mqtt::{self, Mqtt};
use crate::ota;
use crate::shared::*;
use anyhow::Result;
use esp_idf_svc::{
    http::{
        client::{Configuration as HttpConfig, EspHttpConnection},
        Method,
    },
    nvs::EspDefaultNvsPartition,
    wifi::{BlockingWifi, EspWifi},
};
use log::*;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Klien HTTP(S) sederhana dengan sertifikat CA bawaan ESP-IDF (crt bundle).
pub struct Http {
    conn: Option<EspHttpConnection>,
}

impl Http {
    pub fn new() -> Self {
        Self { conn: None }
    }

    fn connection(&mut self) -> Result<&mut EspHttpConnection> {
        if self.conn.is_none() {
            self.conn = Some(EspHttpConnection::new(&HttpConfig {
                timeout: Some(Duration::from_secs(15)),
                crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
                buffer_size: Some(2048),
                buffer_size_tx: Some(1024),
                keep_alive_enable: true,
                ..Default::default()
            })?);
        }
        Ok(self.conn.as_mut().unwrap())
    }

    /// Kirim request; mengembalikan (status, body). Koneksi dibuat ulang bila error.
    pub fn request(&mut self, method: Method, url: &str, body: Option<&[u8]>, extra: &[(&str, &str)]) -> Result<(u16, Vec<u8>)> {
        let r = self.request_inner(method, url, body, extra);
        if r.is_err() {
            self.conn = None;
        }
        r
    }

    fn request_inner(&mut self, method: Method, url: &str, body: Option<&[u8]>, extra: &[(&str, &str)]) -> Result<(u16, Vec<u8>)> {
        let len = body.map(|b| b.len()).unwrap_or(0).to_string();
        let mut headers: Vec<(&str, &str)> = extra.to_vec();
        if body.is_some() {
            headers.push(("Content-Type", "application/json"));
            headers.push(("Content-Length", len.as_str()));
        }
        let conn = self.connection()?;
        conn.initiate_request(method, url, &headers)?;
        if let Some(b) = body {
            conn.write_all(b)?;
        }
        conn.initiate_response()?;
        let status = conn.status();
        let mut out = Vec::new();
        let mut buf = [0u8; 512];
        loop {
            let n = conn.read(&mut buf)?;
            if n == 0 {
                break;
            }
            if out.len() < 32 * 1024 {
                out.extend_from_slice(&buf[..n]);
            }
        }
        Ok((status, out))
    }

    pub fn post_json(&mut self, url: &str, body: &str, extra: &[(&str, &str)]) -> Result<(u16, Vec<u8>)> {
        self.request(Method::Post, url, Some(body.as_bytes()), extra)
    }
}

pub struct NetCtx {
    pub wifi: BlockingWifi<EspWifi<'static>>,
    pub nvs: EspDefaultNvsPartition,
    pub shared: SharedRef,
    pub rx: Receiver<Outgoing>,
    pub acq_tx: Sender<AcqCommand>,
    pub abort: Arc<AtomicBool>,
}

pub fn run(mut ctx: NetCtx) {
    let mut mqtt: Option<Mqtt> = None;
    let boot = Instant::now();
    let mut samples: Vec<(String, SampleRow)> = Vec::new();
    let mut posts: VecDeque<(&'static str, String)> = VecDeque::new();
    let mut last_hb = Instant::now() - Duration::from_secs(60);
    let mut last_flush = Instant::now();
    let mut last_ota = Instant::now();
    let mut last_wifi_try = Instant::now() - Duration::from_secs(60);
    let mut last_mqtt_try = Instant::now() - Duration::from_secs(60);
    let mut last_ack: Option<u64> = None;
    let mut ota_requested = false;
    let mut reboot_pending = false;
    let mut first_ok = false;

    loop {
        // 1. Ambil semua pesan dari thread akuisisi
        while let Ok(msg) = ctx.rx.try_recv() {
            match msg {
                Outgoing::Sample { measurement_id, row } => {
                    samples.push((measurement_id, row));
                    // Batasi memori bila server lama tidak terjangkau (±30 menit data)
                    if samples.len() > 2000 {
                        samples.remove(0);
                    }
                }
                Outgoing::Post { key, body } => posts.push_back((key, body)),
            }
        }
        ctx.shared.lock().unwrap().pending_uploads = (samples.len() + posts.len()) as u32;

        // 2. WiFi
        if !ctx.wifi.is_connected().unwrap_or(false) {
            if last_wifi_try.elapsed() > Duration::from_secs(10) {
                last_wifi_try = Instant::now();
                warn!("WiFi terputus — mencoba menyambung ulang");
                let _ = ctx.wifi.connect().and_then(|_| ctx.wifi.wait_netif_up());
            }
            std::thread::sleep(Duration::from_millis(300));
            continue;
        }

        // 3. MQTT — klien dibuat sekali, selanjutnya menyambung ulang sendiri
        if mqtt.is_none() {
            if last_mqtt_try.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(300));
                continue;
            }
            last_mqtt_try = Instant::now();
            info!("Membuat klien MQTT (heap bebas {} B)", unsafe { esp_idf_svc::sys::esp_get_free_heap_size() });
            match Mqtt::new() {
                Ok(m) => mqtt = Some(m),
                Err(e) => {
                    error!("MQTT tidak bisa dibuat: {e:#}");
                    continue;
                }
            }
        }
        let m = mqtt.as_mut().unwrap();
        m.poll();
        if !m.is_connected() {
            std::thread::sleep(Duration::from_millis(300));
            continue;
        }

        // 4. Perintah dashboard (RPC ThingsBoard). Backend mengirim ulang perintah yang
        //    sama sampai id-nya muncul sebagai last_ack di heartbeat → abaikan duplikat.
        while let Some(cmd) = m.take_command() {
            if Some(cmd.id) == last_ack {
                continue;
            }
            info!("Perintah dari server: {} (id {})", cmd.kind, cmd.id);
            last_ack = Some(cmd.id);
            last_hb = Instant::now() - Duration::from_secs(60); // kirim ack secepatnya
            match cmd.kind.as_str() {
                "start" => match cmd.measurement {
                    Some(meta) => {
                        ctx.abort.store(false, Ordering::SeqCst);
                        let _ = ctx.acq_tx.send(AcqCommand::Start {
                            meta,
                            duration_s: cmd.duration_s.unwrap_or(DEFAULT_DURATION_S),
                            period_ms: cmd.sample_period_ms.unwrap_or(DEFAULT_PERIOD_MS),
                            allow_missing: cmd.allow_missing_sensors.unwrap_or(false),
                        });
                    }
                    None => warn!("Perintah start tanpa metadata"),
                },
                "stop" => ctx.abort.store(true, Ordering::SeqCst),
                "zero_baseline" => {
                    let _ = ctx.acq_tx.send(AcqCommand::ZeroBaseline);
                }
                "set_config" => {
                    if let Some(p) = cmd.sample_period_ms {
                        let _ = ctx.acq_tx.send(AcqCommand::SetPeriod(p));
                    }
                }
                "check_ota" => ota_requested = true,
                // Restart setelah ack terkirim lewat satu heartbeat lagi (hindari reboot berulang)
                "reboot" => reboot_pending = true,
                other => warn!("Perintah tidak dikenal: {other}"),
            }
        }

        // 5. Unggah data mentah (per ±5 detik), harus selesai sebelum hasil akhir dikirim.
        //    Maks. 10 baris per pesan agar nilai telemetry ThingsBoard tetap kecil.
        if !samples.is_empty() && (last_flush.elapsed() >= Duration::from_millis(SAMPLE_FLUSH_MS) || samples.len() >= 10 || !posts.is_empty()) {
            last_flush = Instant::now();
            let mid = samples[0].0.clone();
            let n = samples.iter().take(10).take_while(|(m, _)| *m == mid).count();
            let rows: Vec<&SampleRow> = samples[..n].iter().map(|(_, r)| r).collect();
            let body = json!({ "device_id": DEVICE_ID, "measurement_id": mid, "rows": rows }).to_string();
            match m.send(mqtt::KEY_SAMPLES, &body, true) {
                Ok(_) => {
                    samples.drain(..n);
                }
                Err(e) => warn!("Unggah data mentah gagal (akan diulang): {e:#}"),
            }
        }

        // 6. Event & hasil akhir — berurutan, diulang sampai PUBACK diterima
        if samples.is_empty() {
            while let Some((key, body)) = posts.front() {
                match m.send(key, body, true) {
                    Ok(_) => {
                        info!("Terkirim {key}");
                        posts.pop_front();
                    }
                    Err(e) => {
                        warn!("Kirim {key} gagal (akan diulang): {e:#}");
                        break;
                    }
                }
            }
        }

        // 7. Heartbeat (membawa last_ack sebagai tanda perintah sudah diterima)
        if last_hb.elapsed() >= Duration::from_millis(HEARTBEAT_MS) || reboot_pending {
            last_hb = Instant::now();
            let body = heartbeat_body(&ctx, boot, last_ack);
            match m.send(mqtt::KEY_HEARTBEAT, &body, reboot_pending || !first_ok) {
                Ok(_) => {
                    if !first_ok {
                        // Firmware baru berhasil bicara dengan server → tandai image OTA valid (cegah rollback)
                        first_ok = true;
                        ota::confirm_boot(&ctx.nvs);
                    }
                    if reboot_pending {
                        warn!("Restart atas perintah dashboard");
                        std::thread::sleep(Duration::from_millis(300));
                        esp_idf_svc::hal::reset::restart();
                    }
                }
                Err(e) => warn!("Heartbeat gagal: {e:#}"),
            }
        }

        // 8. OTA (hanya saat idle — tidak pernah memotong perekaman)
        let idle = ctx.shared.lock().unwrap().state == DevState::Idle;
        if idle && samples.is_empty() && posts.is_empty()
            && (ota_requested || last_ota.elapsed() >= Duration::from_secs(OTA_POLL_S))
        {
            ota_requested = false;
            last_ota = Instant::now();
            if let Err(e) = ota::check_and_update(&ctx.shared, &ctx.nvs) {
                warn!("OTA: {e:#}");
            }
        }

        std::thread::sleep(Duration::from_millis(200));
    }
}

fn heartbeat_body(ctx: &NetCtx, boot: Instant, last_ack: Option<u64>) -> String {
    let (ip, mac, rssi) = {
        let w = ctx.wifi.wifi();
        let ip = w.sta_netif().get_ip_info().map(|i| i.ip.to_string()).ok();
        let mac = w.sta_netif().get_mac().ok().map(|m| {
            m.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
        });
        (ip, mac, w.get_rssi().ok())
    };
    let s = ctx.shared.lock().unwrap();
    json!({
        "device_id": DEVICE_ID,
        "fw_title": FW_TITLE,
        "fw_version": FW_VERSION,
        "ip": ip, "mac": mac, "rssi": rssi,
        "uptime_s": boot.elapsed().as_secs(),
        "free_heap": unsafe { esp_idf_svc::sys::esp_get_free_heap_size() },
        "min_free_heap": unsafe { esp_idf_svc::sys::esp_get_minimum_free_heap_size() },
        "state": s.state.as_str(),
        "warmup_remaining_s": s.warmup_remaining_s,
        "measurement_id": s.measurement_id,
        "samples_done": s.samples_done,
        "samples_total": s.samples_total,
        "sensors": s.sensors,
        "live": s.live,
        "last_ack": last_ack,
        "last_error": s.last_error,
        "ota": { "state": s.ota_state, "progress": s.ota_progress, "message": s.ota_message },
        "sample_period_ms": s.sample_period_ms,
        "model_ready": s.model_ready,
        "pending_uploads": s.pending_uploads,
    })
    .to_string()
}
