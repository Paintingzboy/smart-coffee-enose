//! Thread jaringan: WiFi, heartbeat + perintah, unggah data mentah & hasil, OTA.
//!
//! Semua koneksi KELUAR dari ESP32 (HTTPS), jadi perangkat bisa dikendalikan dari
//! dashboard publik walau berada di balik NAT/WiFi kampus.

use crate::config::*;
use crate::ota;
use crate::shared::*;
use anyhow::{bail, Result};
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

fn api(path: &str) -> String {
    format!("{}{}", SERVER_URL.trim_end_matches('/'), path)
}

/// POST ke backend dengan header X-Device-Key; error bila status bukan 2xx.
fn post_backend(http: &mut Http, path: &str, body: &str) -> Result<Vec<u8>> {
    let (status, resp) = http.post_json(&api(path), body, &[("X-Device-Key", DEVICE_KEY)])?;
    if !(200..300).contains(&status) {
        bail!("{path} → HTTP {status}: {}", String::from_utf8_lossy(&resp[..resp.len().min(200)]));
    }
    Ok(resp)
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
    let mut http = Http::new();
    let boot = Instant::now();
    let mut samples: Vec<(String, SampleRow)> = Vec::new();
    let mut posts: VecDeque<(&'static str, String)> = VecDeque::new();
    let mut last_hb = Instant::now() - Duration::from_secs(60);
    let mut last_flush = Instant::now();
    let mut last_ota = Instant::now();
    let mut last_wifi_try = Instant::now() - Duration::from_secs(60);
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
                Outgoing::Post { path, body } => posts.push_back((path, body)),
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

        // 3. Unggah data mentah (per ±5 detik), harus selesai sebelum hasil akhir dikirim
        if !samples.is_empty() && (last_flush.elapsed() >= Duration::from_millis(SAMPLE_FLUSH_MS) || samples.len() >= 20 || !posts.is_empty()) {
            last_flush = Instant::now();
            let mid = samples[0].0.clone();
            let n = samples.iter().take(60).take_while(|(m, _)| *m == mid).count();
            let rows: Vec<&SampleRow> = samples[..n].iter().map(|(_, r)| r).collect();
            let body = json!({ "device_id": DEVICE_ID, "measurement_id": mid, "rows": rows }).to_string();
            match post_backend(&mut http, "/api/device/samples", &body) {
                Ok(_) => {
                    samples.drain(..n);
                }
                Err(e) => warn!("Unggah data mentah gagal (akan diulang): {e:#}"),
            }
        }

        // 4. Event & hasil akhir — berurutan, diulang sampai sukses
        if samples.is_empty() {
            while let Some((path, body)) = posts.front() {
                match post_backend(&mut http, path, body) {
                    Ok(_) => {
                        info!("Terkirim {path}");
                        posts.pop_front();
                    }
                    Err(e) => {
                        warn!("Kirim {path} gagal (akan diulang): {e:#}");
                        break;
                    }
                }
            }
        }

        // 5. Heartbeat + ambil perintah
        if last_hb.elapsed() >= Duration::from_millis(HEARTBEAT_MS) || reboot_pending {
            last_hb = Instant::now();
            let hb = heartbeat(&mut http, &ctx, boot, last_ack);
            if hb.is_ok() && !first_ok {
                // Firmware baru berhasil bicara dengan server → tandai image OTA valid (cegah rollback)
                first_ok = true;
                ota::confirm_boot(&ctx.nvs);
            }
            match hb {
                Ok(Some(cmd)) if Some(cmd.id) != last_ack => {
                    info!("Perintah dari server: {} (id {})", cmd.kind, cmd.id);
                    last_ack = Some(cmd.id);
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
                Ok(_) => {
                    if reboot_pending {
                        warn!("Restart atas perintah dashboard");
                        std::thread::sleep(Duration::from_millis(300));
                        esp_idf_svc::hal::reset::restart();
                    }
                }
                Err(e) => warn!("Heartbeat gagal: {e:#}"),
            }
        }

        // 6. OTA (hanya saat idle — tidak pernah memotong perekaman)
        let idle = ctx.shared.lock().unwrap().state == DevState::Idle;
        if !TB_TOKEN.is_empty() && idle && samples.is_empty() && posts.is_empty()
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

fn heartbeat(http: &mut Http, ctx: &NetCtx, boot: Instant, last_ack: Option<u64>) -> Result<Option<ServerCommand>> {
    let (ip, mac, rssi) = {
        let w = ctx.wifi.wifi();
        let ip = w.sta_netif().get_ip_info().map(|i| i.ip.to_string()).ok();
        let mac = w.sta_netif().get_mac().ok().map(|m| {
            m.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
        });
        (ip, mac, w.get_rssi().ok())
    };
    let body = {
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
    };
    let resp = post_backend(http, "/api/device/heartbeat", &body)?;
    let v: serde_json::Value = serde_json::from_slice(&resp)?;
    Ok(match v.get("command") {
        Some(c) if !c.is_null() => Some(serde_json::from_value(c.clone())?),
        _ => None,
    })
}
