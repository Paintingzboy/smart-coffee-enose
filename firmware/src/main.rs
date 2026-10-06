//! Smart Coffee E-Nose — Firmware ESP32-S3 (Kelas A)
//!
//! Boot → WiFi → NTP → 2 thread:
//!   • acquisition : warm-up, preview live, rekam 500 s, fitur, TinyML (tanpa jaringan)
//!   • net         : MQTT ThingsBoard — heartbeat 5 s, perintah dashboard (RPC), data mentah & hasil; OTA
//!
//! ESP32 cukup dicolokkan ke daya: begitu WiFi tersambung, perangkat muncul "online"
//! di dashboard dan menunggu tombol "Mulai rekam" — walau sensor belum terpasang.
//! Perintah serial `START|...` versi lama tetap didukung sebagai jalur cadangan offline.

mod acquisition;
mod ads1115;
mod config;
mod dht22;
mod ei_model;
mod features;
mod mqtt;
mod net;
mod ota;
mod shared;

use anyhow::{anyhow, Result};
use config::*;
use esp_idf_hal::{
    gpio::AnyIOPin,
    i2c::{I2cConfig, I2cDriver},
    peripherals::Peripherals,
    units::*,
};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::EspDefaultNvsPartition,
    sntp::{EspSntp, SyncStatus},
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use log::*;
use shared::*;
use std::io::BufRead;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    if let Err(e) = run() {
        error!("Fatal: {e:#} — restart dalam 10 detik");
        thread::sleep(Duration::from_secs(10));
        esp_idf_svc::hal::reset::restart();
    }
}

fn run() -> Result<()> {
    info!("Smart Coffee E-Nose {FW_TITLE} v{FW_VERSION} — {DEVICE_ID} → {TB_MQTT_URL}");
    let model = ei_model::EiModel::new();
    let shared = Shared::new(model.is_ready());

    let p = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // I2C ADS1115 (SDA=GPIO20, SCL=GPIO21) — pull-up 4,7 kΩ eksternal disarankan
    let i2c = I2cDriver::new(p.i2c0, p.pins.gpio20, p.pins.gpio21, &I2cConfig::new().baudrate(400.kHz().into()))?;

    // DHT22 di GPIO4 — kegagalan inisialisasi tidak menghentikan firmware
    let dht_pin: AnyIOPin<'static> = p.pins.gpio4.into();
    let dht = match dht22::Dht22::new(dht_pin) {
        Ok(d) => Some(d),
        Err(e) => {
            warn!("DHT22 tidak siap: {e:#}");
            None
        }
    };

    // WiFi — koneksi awal dicoba, tetapi tidak fatal (thread net akan mencoba ulang)
    let mut wifi = BlockingWifi::wrap(EspWifi::new(p.modem, sys_loop.clone(), Some(nvs.clone()))?, sys_loop)?;
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID.try_into().map_err(|_| anyhow!("SSID terlalu panjang"))?,
        password: WIFI_PASS.try_into().map_err(|_| anyhow!("Password terlalu panjang"))?,
        auth_method: if WIFI_PASS.is_empty() { AuthMethod::None } else { AuthMethod::WPA2Personal },
        ..Default::default()
    }))?;
    wifi.start()?;
    info!("Menghubungkan WiFi '{WIFI_SSID}'…");
    match wifi.connect().and_then(|_| wifi.wait_netif_up()) {
        Ok(_) => info!("WiFi tersambung: {:?}", wifi.wifi().sta_netif().get_ip_info().map(|i| i.ip)),
        Err(e) => warn!("WiFi belum tersambung ({e}) — akan dicoba ulang"),
    }

    // NTP untuk timestamp absolut (tidak fatal)
    let sntp = EspSntp::new_default()?;
    let t0 = Instant::now();
    while sntp.get_sync_status() != SyncStatus::Completed && t0.elapsed() < Duration::from_secs(20) {
        thread::sleep(Duration::from_millis(250));
    }
    if sntp.get_sync_status() == SyncStatus::Completed {
        info!("Waktu UTC tersinkronisasi");
    } else {
        warn!("NTP belum sinkron — timestamp absolut mungkin salah sampai sinkron");
    }

    // sync_channel memesan memori untuk SEMUA slot sejak awal (±160 B/slot) — jangan besar-besar,
    // RAM internal hanya ±280 KB dan MQTT+TLS butuh ±50 KB. 120 slot = 2 menit data @1 Hz.
    let (out_tx, out_rx) = mpsc::sync_channel::<Outgoing>(120);
    let (cmd_tx, cmd_rx) = mpsc::channel::<AcqCommand>();
    let abort = Arc::new(AtomicBool::new(false));

    // Jalur cadangan: perintah serial START|ID|DAQ|Batch|Group|Coffee|Aliquot|Species|BeanState
    let serial_tx = cmd_tx.clone();
    thread::Builder::new().stack_size(6 * 1024).spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            let parts: Vec<&str> = line.trim().split('|').collect();
            if parts.len() == 9 && parts[0] == "START" && parts[1..].iter().all(|v| !v.is_empty()) {
                let meta = MeasurementMeta {
                    measurement_id: parts[1].into(), daq_id: parts[2].into(), batch_id: parts[3].into(),
                    group_id: parts[4].into(), coffee_id: parts[5].into(), aliquot_id: parts[6].into(),
                    species: parts[7].into(), bean_state: parts[8].into(),
                };
                let _ = serial_tx.send(AcqCommand::Start { meta, duration_s: DEFAULT_DURATION_S, period_ms: DEFAULT_PERIOD_MS, allow_missing: false });
            } else if !line.trim().is_empty() {
                warn!("Format serial: START|ID|DAQ|Batch|Group|Coffee|Aliquot|Species|BeanState");
            }
        }
    })?;

    let acq = acquisition::AcqCtx { i2c, dht, model, shared: shared.clone(), rx: cmd_rx, tx: out_tx, abort: abort.clone() };
    let acq_handle = thread::Builder::new().name("acq".into()).stack_size(16 * 1024).spawn(move || acquisition::run(acq))?;

    let net_ctx = net::NetCtx { wifi, nvs, shared, rx: out_rx, acq_tx: cmd_tx, abort };
    let net_handle = thread::Builder::new().name("net".into()).stack_size(24 * 1024).spawn(move || net::run(net_ctx))?;

    // Thread utama menjaga SNTP tetap hidup
    let _keep = sntp;
    let _ = acq_handle.join();
    let _ = net_handle.join();
    Err(anyhow!("thread utama berhenti"))
}
