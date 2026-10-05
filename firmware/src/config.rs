//! Konfigurasi firmware. Nilai rahasia TIDAK ditulis di kode — diisi saat build
//! lewat environment variable, contoh (PowerShell):
//!
//!   $env:WIFI_SSID="Lab-IoT"; $env:WIFI_PASS="..."; $env:SERVER_URL="https://xxx.up.railway.app"
//!   $env:DEVICE_KEY="..."; $env:DEVICE_ID="DAQ01"; $env:TB_TOKEN="..."; cargo build --release
//!
//! Bila variabel tidak diset, nilai default di bawah yang dipakai.

macro_rules! env_or {
    ($name:literal, $default:expr) => {
        match option_env!($name) {
            Some(v) => v,
            None => $default,
        }
    };
}

/// WiFi lab (WPA2-Personal). Untuk WiFi tanpa password, kosongkan WIFI_PASS.
pub const WIFI_SSID: &str = env_or!("WIFI_SSID", "TeknologiIoT");
pub const WIFI_PASS: &str = env_or!("WIFI_PASS", "passwordwiFi");

/// URL publik backend (Railway). Untuk uji lokal: http://192.168.x.x:3000
pub const SERVER_URL: &str = env_or!("SERVER_URL", "http://192.168.110.221:3000");
/// Harus sama dengan DEVICE_API_KEY di backend.
pub const DEVICE_KEY: &str = env_or!("DEVICE_KEY", "dev-device-key");
/// ID perangkat — harus sama dengan nama device di ThingsBoard (DAQ01..DAQ05).
pub const DEVICE_ID: &str = env_or!("DEVICE_ID", "DAQ01");

/// ThingsBoard untuk OTA. TB_TOKEN = access token device (Devices → Copy access token).
/// Bila kosong, fitur OTA nonaktif.
pub const TB_URL: &str = env_or!("TB_URL", "https://eu.thingsboard.cloud");
pub const TB_TOKEN: &str = env_or!("TB_TOKEN", "");

/// Judul firmware — harus sama dengan "fw_title" paket OTA di ThingsBoard.
pub const FW_TITLE: &str = env_or!("FW_TITLE", "smart-coffee-enose");
/// Versi firmware diambil dari `version` di Cargo.toml. NAIKKAN setiap rilis OTA.
pub const FW_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Warm-up sensor MOS setelah boot (detik). SOP handbook menyarankan ≥30 menit per sesi;
/// nilai ini hanya batas minimum sebelum perekaman diizinkan. Ubah via env WARMUP_S.
pub const WARMUP_SECONDS: u64 = parse_u64(env_or!("WARMUP_S", "60"));

/// Default rekaman (dapat diubah dari dashboard per pengukuran).
pub const DEFAULT_DURATION_S: u32 = 500;
pub const DEFAULT_PERIOD_MS: u32 = 1000;

/// Interval komunikasi.
pub const HEARTBEAT_MS: u64 = 3000;
pub const SAMPLE_FLUSH_MS: u64 = 5000;
pub const OTA_POLL_S: u64 = 60;

// Pin (didefinisikan langsung di main.rs karena tipe peripheral esp-idf-hal per pin):
//   I2C SDA = GPIO20, I2C SCL = GPIO21, DHT22 DATA = GPIO4

const fn parse_u64(s: &str) -> u64 {
    let b = s.as_bytes();
    let mut i = 0;
    let mut v = 0u64;
    while i < b.len() {
        assert!(b[i] >= b'0' && b[i] <= b'9', "WARMUP_S harus angka");
        v = v * 10 + (b[i] - b'0') as u64;
        i += 1;
    }
    v
}
