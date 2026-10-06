//! State bersama antar-thread dan pesan antar-thread.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DevState {
    Booting,
    WarmingUp,
    Idle,
    Recording,
    Uploading,
    Ota,
}

impl DevState {
    pub fn as_str(self) -> &'static str {
        match self {
            DevState::Booting => "booting",
            DevState::WarmingUp => "warming_up",
            DevState::Idle => "idle",
            DevState::Recording => "recording",
            DevState::Uploading => "uploading",
            DevState::Ota => "ota",
        }
    }
}

#[derive(Clone, Default, Serialize)]
pub struct SensorStatus {
    pub ads_a: bool,
    pub ads_b: bool,
    pub dht: bool,
}

#[derive(Clone, Serialize)]
pub struct LiveReading {
    pub v: [Option<f32>; 8],
    pub temp_c: Option<f32>,
    pub rh_pct: Option<f32>,
}

pub struct Shared {
    pub state: DevState,
    pub warmup_remaining_s: u32,
    pub measurement_id: Option<String>,
    pub samples_done: u32,
    pub samples_total: u32,
    pub sensors: SensorStatus,
    pub live: Option<LiveReading>,
    pub last_error: Option<String>,
    pub ota_state: String,
    pub ota_progress: f32,
    pub ota_message: Option<String>,
    pub sample_period_ms: u32,
    pub model_ready: bool,
    pub pending_uploads: u32,
}

pub type SharedRef = Arc<Mutex<Shared>>;

impl Shared {
    pub fn new(model_ready: bool) -> SharedRef {
        Arc::new(Mutex::new(Self {
            state: DevState::Booting,
            warmup_remaining_s: 0,
            measurement_id: None,
            samples_done: 0,
            samples_total: 0,
            sensors: SensorStatus::default(),
            live: None,
            last_error: None,
            ota_state: "IDLE".into(),
            ota_progress: 0.0,
            ota_message: None,
            sample_period_ms: crate::config::DEFAULT_PERIOD_MS,
            model_ready,
            pending_uploads: 0,
        }))
    }
}

/// Metadata pengukuran dari dashboard.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MeasurementMeta {
    pub measurement_id: String,
    pub daq_id: String,
    pub batch_id: String,
    pub group_id: String,
    pub coffee_id: String,
    pub aliquot_id: String,
    pub species: String,
    pub bean_state: String,
}

/// Perintah yang diterima dari server (lewat RPC ThingsBoard).
#[derive(Clone, Debug, Deserialize)]
pub struct ServerCommand {
    pub id: u64,
    #[serde(rename = "type")]
    pub kind: String,
    pub measurement: Option<MeasurementMeta>,
    pub duration_s: Option<u32>,
    pub sample_period_ms: Option<u32>,
    pub allow_missing_sensors: Option<bool>,
}

/// Perintah untuk thread akuisisi.
pub enum AcqCommand {
    Start { meta: MeasurementMeta, duration_s: u32, period_ms: u32, allow_missing: bool },
    ZeroBaseline,
    SetPeriod(u32),
}

/// Satu baris data mentah (kolom = CSV handbook 17.4).
#[derive(Clone, Serialize)]
pub struct SampleRow {
    pub i: u32,
    pub t_s: f32,
    pub ts: String,
    pub raw: [Option<u16>; 8],
    pub v: [Option<f32>; 8],
    pub temp_c: Option<f32>,
    pub rh_pct: Option<f32>,
    pub dht_age_s: Option<u32>,
}

/// Pesan dari thread akuisisi → thread jaringan.
pub enum Outgoing {
    Sample { measurement_id: String, row: SampleRow },
    /// Kunci telemetry MQTT + body JSON (event / hasil akhir). Dikirim berurutan, diulang sampai berhasil.
    Post { key: &'static str, body: String },
}
