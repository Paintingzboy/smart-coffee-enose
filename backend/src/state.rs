//! State aplikasi bersama: konfigurasi, penyimpanan, ThingsBoard, dan registri
//! perangkat yang sedang online (disimpan di memori — heartbeat 3 detik terlalu
//! sering untuk ditulis ke database).

use crate::config::Config;
use crate::models::*;
use crate::store::Store;
use crate::thingsboard::ThingsBoard;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub const ONLINE_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_LIVE_HISTORY: usize = 300;
const MAX_SAMPLES: usize = 4000;
const MAX_EVENTS: usize = 40;

pub type Shared = Arc<AppState>;

pub struct AppState {
    pub cfg: Config,
    pub store: Store,
    pub tb: Option<ThingsBoard>,
    pub devices: RwLock<HashMap<String, DeviceLive>>,
    /// Sesi yang belum selesai: measurement_id → info pemantauan.
    pub active: RwLock<HashMap<String, ActiveSession>>,
    pub list_cache: RwLock<Option<(Instant, Arc<Vec<MeasurementRow>>)>>,
    pub health_cache: RwLock<Option<(Instant, serde_json::Value)>>,
    pub started: Instant,
    cmd_seq: AtomicU64,
}

#[derive(Debug, Clone)]
pub struct ActiveSession {
    pub device_id: String,
    pub queued_at: Instant,
    pub started: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LivePoint {
    pub at: DateTime<Utc>,
    pub v: Vec<Option<f64>>,
    pub temp_c: Option<f64>,
    pub rh_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventLog {
    pub at: DateTime<Utc>,
    pub event: String,
    pub measurement_id: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeviceLive {
    pub hb: Heartbeat,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub last_seen_at: Instant,
    pub pending_command: Option<Command>,
    pub live_history: VecDeque<LivePoint>,
    /// Data mentah pengukuran aktif/terakhir (untuk grafik real-time).
    pub samples_measurement: Option<String>,
    pub samples: VecDeque<SampleRow>,
    pub zero_baseline: Option<(DateTime<Utc>, Vec<Option<f64>>)>,
    pub events: VecDeque<EventLog>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceSummary {
    pub device_id: String,
    pub online: bool,
    pub last_seen: DateTime<Utc>,
    pub seconds_since_seen: f64,
    pub first_seen: DateTime<Utc>,
    #[serde(flatten)]
    pub hb: Heartbeat,
    pub pending_command: Option<Command>,
    pub zero_baseline: Option<Vec<Option<f64>>>,
    pub zero_baseline_at: Option<DateTime<Utc>>,
}

impl DeviceLive {
    pub fn new(hb: Heartbeat) -> Self {
        let now = Utc::now();
        Self {
            hb,
            first_seen: now,
            last_seen: now,
            last_seen_at: Instant::now(),
            pending_command: None,
            live_history: VecDeque::new(),
            samples_measurement: None,
            samples: VecDeque::new(),
            zero_baseline: None,
            events: VecDeque::new(),
        }
    }

    pub fn online(&self) -> bool {
        self.last_seen_at.elapsed() < ONLINE_TIMEOUT
    }

    pub fn summary(&self) -> DeviceSummary {
        DeviceSummary {
            device_id: self.hb.device_id.clone(),
            online: self.online(),
            last_seen: self.last_seen,
            seconds_since_seen: self.last_seen_at.elapsed().as_secs_f64(),
            first_seen: self.first_seen,
            hb: self.hb.clone(),
            pending_command: self.pending_command.clone(),
            zero_baseline: self.zero_baseline.as_ref().map(|z| z.1.clone()),
            zero_baseline_at: self.zero_baseline.as_ref().map(|z| z.0),
        }
    }

    pub fn push_live(&mut self, live: &LiveReading) {
        self.live_history.push_back(LivePoint {
            at: Utc::now(),
            v: live.v.clone(),
            temp_c: live.temp_c,
            rh_pct: live.rh_pct,
        });
        while self.live_history.len() > MAX_LIVE_HISTORY {
            self.live_history.pop_front();
        }
    }

    pub fn push_samples(&mut self, measurement_id: &str, rows: &[SampleRow]) {
        if self.samples_measurement.as_deref() != Some(measurement_id) {
            self.samples_measurement = Some(measurement_id.to_string());
            self.samples.clear();
        }
        for r in rows {
            if self.samples.back().map(|b| r.i > b.i).unwrap_or(true) {
                self.samples.push_back(r.clone());
            } else if !self.samples.iter().any(|x| x.i == r.i) {
                // datang terlambat (retry) — sisipkan sesuai urutan
                let pos = self.samples.iter().position(|x| x.i > r.i).unwrap_or(self.samples.len());
                self.samples.insert(pos, r.clone());
            }
        }
        while self.samples.len() > MAX_SAMPLES {
            self.samples.pop_front();
        }
    }

    pub fn log(&mut self, event: &str, measurement_id: Option<&str>, message: Option<&str>) {
        self.events.push_front(EventLog {
            at: Utc::now(),
            event: event.into(),
            measurement_id: measurement_id.map(str::to_string),
            message: message.map(str::to_string),
        });
        self.events.truncate(MAX_EVENTS);
    }
}

impl AppState {
    pub fn new(cfg: Config, store: Store, tb: Option<ThingsBoard>) -> Self {
        // ID perintah dimulai dari waktu unix (ms) agar tetap unik walau server restart.
        let seed = Utc::now().timestamp_millis() as u64;
        Self {
            cfg,
            store,
            tb,
            devices: RwLock::new(HashMap::new()),
            active: RwLock::new(HashMap::new()),
            list_cache: RwLock::new(None),
            health_cache: RwLock::new(None),
            started: Instant::now(),
            cmd_seq: AtomicU64::new(seed),
        }
    }

    pub fn next_command_id(&self) -> u64 {
        self.cmd_seq.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub async fn invalidate_list(&self) {
        *self.list_cache.write().await = None;
    }

    /// Daftar pengukuran dengan cache 3 detik (dashboard melakukan polling).
    pub async fn measurements(&self) -> anyhow::Result<Arc<Vec<MeasurementRow>>> {
        if let Some((t, rows)) = self.list_cache.read().await.as_ref() {
            if t.elapsed() < Duration::from_secs(3) {
                return Ok(rows.clone());
            }
        }
        let rows = Arc::new(self.store.list().await?);
        *self.list_cache.write().await = Some((Instant::now(), rows.clone()));
        Ok(rows)
    }
}
