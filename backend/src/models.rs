//! Tipe data yang dipertukarkan antara ESP32, backend, database, dan dashboard.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const SENSOR_COUNT: usize = 8;
/// Urutan kanal identik dengan firmware & handbook (S1..S8).
pub const SENSOR_KEYS: [&str; SENSOR_COUNT] =
    ["mq3", "mq6", "mq7", "mq135", "tgs2600", "tgs2602", "tgs2611", "tgs2620"];

// ─────────────────────────── Sesi pengukuran ───────────────────────────

/// Satu baris metadata pengukuran (measurement_sessions). Dibuat saat operator
/// menekan "Mulai rekam", diperbarui oleh event dari ESP32.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Session {
    pub measurement_id: String,
    pub device_id: String,
    pub daq_id: Option<String>,
    pub batch_id: Option<String>,
    pub group_id: Option<String>,
    pub coffee_id: Option<String>,
    pub aliquot_id: Option<String>,
    pub species: Option<String>,
    pub bean_state: Option<String>,
    pub roast_level: Option<String>,
    pub mass_g: Option<f64>,
    pub operator_name: Option<String>,
    pub protocol_ver: Option<String>,
    pub run_order: Option<i32>,
    pub heating_s: Option<i32>,
    pub duration_s: i32,
    pub sample_period_ms: i32,
    pub warmup_min: Option<i32>,
    pub room_t: Option<f64>,
    pub room_rh: Option<f64>,
    pub divider_ratio: Option<f64>,
    pub notes: Option<String>,
    pub qc_flag: String,
    /// queued | recording | completed | failed | cancelled | interrupted
    pub status: String,
    pub dry_run: bool,
    pub error: Option<String>,
    pub samples_received: i32,
    pub created_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// Payload hasil akhir dari ESP32 (format lama firmware Kelas A dipertahankan).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reading {
    pub device_id: String,
    pub fw_version: Option<String>,
    pub measurement_id: String,
    pub daq_id: Option<String>,
    pub batch_id: Option<String>,
    pub group_id: Option<String>,
    pub coffee_id: Option<String>,
    pub aliquot_id: Option<String>,
    pub species: Option<String>,
    pub bean_state: Option<String>,
    pub predicted_class: Option<String>,
    pub confidence: Option<f64>,
    pub model_version: Option<String>,
    pub temperature: Option<f64>,
    pub humidity: Option<f64>,
    pub sample_count: Option<i32>,
    pub sample_duration_s: Option<f64>,
    pub inference_time_ms: Option<f64>,
    #[serde(default)]
    pub sensor_late_mean: Vec<Option<f64>>,
    #[serde(default)]
    pub sensor_peak_abs: Vec<Option<f64>>,
    pub seq: Option<i64>,
    pub timestamp: Option<String>,
}

/// Gabungan sesi + hasil, dikirim ke dashboard.
#[derive(Debug, Clone, Serialize)]
pub struct MeasurementRow {
    #[serde(flatten)]
    pub session: Session,
    pub result: Option<ResultView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResultView {
    pub predicted_class: Option<String>,
    pub confidence: Option<f64>,
    pub model_version: Option<String>,
    pub fw_version: Option<String>,
    pub temperature: Option<f64>,
    pub humidity: Option<f64>,
    pub sample_count: Option<i32>,
    pub sample_duration_s: Option<f64>,
    pub inference_time_ms: Option<f64>,
    pub sensor_late_mean: Vec<Option<f64>>,
    pub sensor_peak_abs: Vec<Option<f64>>,
    pub reading_timestamp: Option<String>,
}

impl From<&Reading> for ResultView {
    fn from(r: &Reading) -> Self {
        Self {
            predicted_class: r.predicted_class.clone(),
            confidence: r.confidence,
            model_version: r.model_version.clone(),
            fw_version: r.fw_version.clone(),
            temperature: r.temperature,
            humidity: r.humidity,
            sample_count: r.sample_count,
            sample_duration_s: r.sample_duration_s,
            inference_time_ms: r.inference_time_ms,
            sensor_late_mean: r.sensor_late_mean.clone(),
            sensor_peak_abs: r.sensor_peak_abs.clone(),
            reading_timestamp: r.timestamp.clone(),
        }
    }
}

// ─────────────────────────── Data mentah ───────────────────────────

/// Satu baris data mentah (1 detik) — sama dengan kolom CSV handbook bagian 17.4.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleRow {
    /// indeks sampel 0..N-1
    pub i: i32,
    pub t_s: f64,
    pub ts: Option<String>,
    pub raw: Vec<Option<i32>>,
    pub v: Vec<Option<f64>>,
    pub temp_c: Option<f64>,
    pub rh_pct: Option<f64>,
    pub dht_age_s: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct SampleBatch {
    pub device_id: String,
    pub measurement_id: String,
    pub rows: Vec<SampleRow>,
}

// ─────────────────────────── Perangkat & perintah ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SensorStatus {
    pub ads_a: bool,
    pub ads_b: bool,
    pub dht: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveReading {
    pub v: Vec<Option<f64>>,
    pub temp_c: Option<f64>,
    pub rh_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OtaProgress {
    pub state: Option<String>,
    pub progress: Option<f64>,
    pub message: Option<String>,
}

/// Heartbeat yang dikirim ESP32 setiap ±3 detik.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    pub device_id: String,
    pub fw_title: Option<String>,
    pub fw_version: Option<String>,
    pub ip: Option<String>,
    pub mac: Option<String>,
    pub rssi: Option<i32>,
    pub uptime_s: Option<u64>,
    pub free_heap: Option<u64>,
    pub min_free_heap: Option<u64>,
    /// booting | warming_up | idle | recording | uploading | ota | error
    pub state: String,
    pub warmup_remaining_s: Option<u32>,
    pub measurement_id: Option<String>,
    pub samples_done: Option<u32>,
    pub samples_total: Option<u32>,
    #[serde(default)]
    pub sensors: SensorStatus,
    pub live: Option<LiveReading>,
    pub last_ack: Option<u64>,
    pub last_error: Option<String>,
    pub ota: Option<OtaProgress>,
    pub sample_period_ms: Option<u32>,
    pub model_ready: Option<bool>,
    pub pending_uploads: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMeasurement {
    pub measurement_id: String,
    pub daq_id: String,
    pub batch_id: String,
    pub group_id: String,
    pub coffee_id: String,
    pub aliquot_id: String,
    pub species: String,
    pub bean_state: String,
}

/// Perintah dari dashboard → ESP32 (diambil ESP32 lewat respons heartbeat).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub id: u64,
    /// start | stop | zero_baseline | set_config | check_ota | reboot
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measurement: Option<CommandMeasurement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_s: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_period_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_missing_sensors: Option<bool>,
    pub issued_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct DeviceEvent {
    pub device_id: String,
    pub measurement_id: Option<String>,
    /// started | completed | failed | cancelled | zero_baseline | ota
    pub event: String,
    pub message: Option<String>,
    pub values: Option<Vec<Option<f64>>>,
}
