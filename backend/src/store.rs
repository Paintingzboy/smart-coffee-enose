//! Penyimpanan data: Azure SQL (tiberius) atau memori (fallback saat DB belum
//! dikonfigurasi — berguna untuk uji lokal tanpa Azure).
//!
//! Dataset kelas berukuran kecil (±300 pengukuran), sehingga filter/sort/agregasi
//! dilakukan di Rust setelah membaca seluruh sesi. Query SQL dibuat sesederhana
//! mungkin agar mudah diaudit.

use crate::config::DbConfig;
use crate::models::*;
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tiberius::{AuthMethod, Client, Config, EncryptionLevel, Query, Row};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

pub enum Store {
    Sql(SqlStore),
    Mem(MemStore),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatusChange {
    Recording,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl StatusChange {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recording => "recording",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }
    /// Status asal yang boleh berpindah ke status ini.
    fn allowed_from(self) -> &'static [&'static str] {
        match self {
            Self::Recording => &["queued"],
            Self::Completed => &["queued", "recording", "failed", "interrupted", "cancelled"],
            _ => &["queued", "recording"],
        }
    }
}

impl Store {
    pub fn kind(&self) -> &'static str {
        match self {
            Store::Sql(_) => "azure-sql",
            Store::Mem(_) => "memory",
        }
    }

    pub async fn migrate(&self) -> Result<()> {
        match self {
            Store::Sql(s) => s.migrate().await,
            Store::Mem(_) => Ok(()),
        }
    }

    pub async fn ping(&self) -> Result<Duration> {
        let t = Instant::now();
        match self {
            Store::Sql(s) => s.ping().await?,
            Store::Mem(_) => {}
        }
        Ok(t.elapsed())
    }

    pub async fn counts(&self) -> Result<(i64, i64, i64)> {
        match self {
            Store::Sql(s) => s.counts().await,
            Store::Mem(m) => {
                let d = m.data.lock().unwrap();
                Ok((
                    d.sessions.len() as i64,
                    d.readings.len() as i64,
                    d.samples.values().map(|v| v.len() as i64).sum(),
                ))
            }
        }
    }

    pub async fn insert_session(&self, s: &Session) -> Result<()> {
        match self {
            Store::Sql(db) => db.insert_session(s).await,
            Store::Mem(m) => {
                let mut d = m.data.lock().unwrap();
                if d.sessions.iter().any(|x| x.measurement_id == s.measurement_id) {
                    return Err(anyhow!("Measurement_ID {} sudah ada", s.measurement_id));
                }
                let mut s = s.clone();
                s.created_at = Some(Utc::now());
                d.sessions.push(s);
                Ok(())
            }
        }
    }

    pub async fn set_status(&self, id: &str, change: StatusChange, error: Option<&str>) -> Result<()> {
        match self {
            Store::Sql(db) => db.set_status(id, change, error).await,
            Store::Mem(m) => {
                let mut d = m.data.lock().unwrap();
                if let Some(s) = d.sessions.iter_mut().find(|x| x.measurement_id == id) {
                    if change.allowed_from().contains(&s.status.as_str()) {
                        s.status = change.as_str().into();
                        if let Some(e) = error {
                            s.error = Some(e.into());
                        }
                        if change == StatusChange::Recording {
                            s.started_at.get_or_insert(Utc::now());
                        } else {
                            s.finished_at = Some(Utc::now());
                        }
                    }
                }
                Ok(())
            }
        }
    }

    pub async fn set_qc(&self, id: &str, qc: &str, notes: Option<&str>) -> Result<bool> {
        match self {
            Store::Sql(db) => db.set_qc(id, qc, notes).await,
            Store::Mem(m) => {
                let mut d = m.data.lock().unwrap();
                match d.sessions.iter_mut().find(|x| x.measurement_id == id) {
                    Some(s) => {
                        s.qc_flag = qc.into();
                        if let Some(n) = notes {
                            s.notes = Some(n.into());
                        }
                        Ok(true)
                    }
                    None => Ok(false),
                }
            }
        }
    }

    pub async fn insert_samples(&self, id: &str, rows: &[SampleRow]) -> Result<()> {
        match self {
            Store::Sql(db) => db.insert_samples(id, rows).await,
            Store::Mem(m) => {
                let mut d = m.data.lock().unwrap();
                let entry = d.samples.entry(id.to_string()).or_default();
                for r in rows {
                    entry.entry(r.i).or_insert_with(|| r.clone());
                }
                let n = entry.len() as i32;
                if let Some(s) = d.sessions.iter_mut().find(|x| x.measurement_id == id) {
                    s.samples_received = n;
                }
                Ok(())
            }
        }
    }

    /// Simpan hasil akhir. Mengembalikan false bila Measurement_ID sudah pernah disimpan
    /// (ESP32 mengirim ulang setelah koneksi putus — idempoten).
    pub async fn insert_reading(&self, r: &Reading) -> Result<bool> {
        match self {
            Store::Sql(db) => db.insert_reading(r).await,
            Store::Mem(m) => {
                let mut d = m.data.lock().unwrap();
                if d.readings.contains_key(&r.measurement_id) {
                    return Ok(false);
                }
                d.readings.insert(r.measurement_id.clone(), r.clone());
                if !d.sessions.iter().any(|s| s.measurement_id == r.measurement_id) {
                    d.sessions.push(session_from_reading(r));
                }
                Ok(true)
            }
        }
    }

    pub async fn list(&self) -> Result<Vec<MeasurementRow>> {
        match self {
            Store::Sql(db) => db.list().await,
            Store::Mem(m) => {
                let d = m.data.lock().unwrap();
                Ok(d.sessions
                    .iter()
                    .map(|s| MeasurementRow {
                        session: s.clone(),
                        result: d.readings.get(&s.measurement_id).map(ResultView::from),
                    })
                    .collect())
            }
        }
    }

    pub async fn samples(&self, id: &str) -> Result<Vec<SampleRow>> {
        match self {
            Store::Sql(db) => db.samples(id).await,
            Store::Mem(m) => {
                let d = m.data.lock().unwrap();
                Ok(d.samples.get(id).map(|v| v.values().cloned().collect()).unwrap_or_default())
            }
        }
    }
}

fn session_from_reading(r: &Reading) -> Session {
    Session {
        measurement_id: r.measurement_id.clone(),
        device_id: r.device_id.clone(),
        daq_id: r.daq_id.clone(),
        batch_id: r.batch_id.clone(),
        group_id: r.group_id.clone(),
        coffee_id: r.coffee_id.clone(),
        aliquot_id: r.aliquot_id.clone(),
        species: r.species.clone(),
        bean_state: r.bean_state.clone(),
        duration_s: r.sample_duration_s.map(|d| d.round() as i32).unwrap_or(500),
        sample_period_ms: 1000,
        qc_flag: "OK".into(),
        status: "completed".into(),
        notes: Some("Dibuat otomatis dari hasil ESP32 (tanpa sesi dashboard)".into()),
        created_at: Some(Utc::now()),
        finished_at: Some(Utc::now()),
        ..Default::default()
    }
}

// ═════════════════════════════ Memory store ═════════════════════════════

#[derive(Default)]
pub struct MemData {
    sessions: Vec<Session>,
    readings: HashMap<String, Reading>,
    samples: HashMap<String, BTreeMap<i32, SampleRow>>,
}

#[derive(Default)]
pub struct MemStore {
    data: Mutex<MemData>,
}

// ═════════════════════════════ Azure SQL store ═════════════════════════════

type Conn = Client<Compat<TcpStream>>;

pub struct SqlStore {
    cfg: DbConfig,
    idle: Mutex<Vec<(Conn, Instant)>>,
    limit: Semaphore,
}

struct Pooled<'a> {
    store: &'a SqlStore,
    conn: Option<Conn>,
    _permit: tokio::sync::SemaphorePermit<'a>,
}

impl Pooled<'_> {
    fn conn(&mut self) -> &mut Conn {
        self.conn.as_mut().expect("koneksi sudah dibuang")
    }
    fn discard(&mut self) {
        self.conn = None;
    }
}

impl Drop for Pooled<'_> {
    fn drop(&mut self) {
        if let Some(c) = self.conn.take() {
            self.store.idle.lock().unwrap().push((c, Instant::now()));
        }
    }
}

/// Jalankan `$body` dengan koneksi dari pool; koneksi dibuang bila terjadi error
/// (mis. koneksi Azure yang sudah ditutup server), sehingga request berikutnya
/// otomatis membuat koneksi baru.
macro_rules! with_conn {
    ($self:ident, $c:ident, $body:block) => {{
        let mut guard = $self.get().await?;
        let r: anyhow::Result<_> = async {
            let $c = guard.conn();
            $body
        }
        .await;
        if r.is_err() {
            guard.discard();
        }
        r
    }};
}

const MIGRATIONS: &[&str] = &[
    r#"IF OBJECT_ID(N'dbo.sensor_readings', N'U') IS NULL
CREATE TABLE dbo.sensor_readings (
    id INT IDENTITY(1,1) PRIMARY KEY,
    device_id NVARCHAR(50) NOT NULL,
    fw_version NVARCHAR(20) NULL,
    measurement_id NVARCHAR(64) NOT NULL,
    daq_id NVARCHAR(20) NULL,
    batch_id NVARCHAR(20) NULL,
    group_id NVARCHAR(20) NULL,
    coffee_id NVARCHAR(20) NULL,
    aliquot_id NVARCHAR(20) NULL,
    species NVARCHAR(20) NULL,
    bean_state NVARCHAR(20) NULL,
    predicted_class NVARCHAR(50) NULL,
    confidence FLOAT NULL,
    model_version NVARCHAR(50) NULL,
    temperature FLOAT NULL,
    humidity FLOAT NULL,
    sample_count INT NULL,
    sample_duration_s FLOAT NULL,
    inference_time_ms FLOAT NULL,
    sensor_late_mean NVARCHAR(MAX) NULL,
    sensor_peak_abs NVARCHAR(MAX) NULL,
    seq BIGINT NULL,
    reading_timestamp NVARCHAR(40) NULL,
    created_at DATETIME2 DEFAULT SYSUTCDATETIME()
)"#,
    r#"IF NOT EXISTS (SELECT 1 FROM sys.indexes WHERE name = 'IX_sensor_readings_measurement_id')
CREATE INDEX IX_sensor_readings_measurement_id ON dbo.sensor_readings (measurement_id)"#,
    r#"IF OBJECT_ID(N'dbo.measurement_sessions', N'U') IS NULL
CREATE TABLE dbo.measurement_sessions (
    measurement_id NVARCHAR(64) NOT NULL PRIMARY KEY,
    device_id NVARCHAR(50) NOT NULL,
    daq_id NVARCHAR(20) NULL,
    batch_id NVARCHAR(20) NULL,
    group_id NVARCHAR(20) NULL,
    coffee_id NVARCHAR(20) NULL,
    aliquot_id NVARCHAR(20) NULL,
    species NVARCHAR(20) NULL,
    bean_state NVARCHAR(20) NULL,
    roast_level NVARCHAR(20) NULL,
    mass_g FLOAT NULL,
    operator_name NVARCHAR(50) NULL,
    protocol_ver NVARCHAR(20) NULL,
    run_order INT NULL,
    heating_s INT NULL,
    duration_s INT NOT NULL,
    sample_period_ms INT NOT NULL,
    warmup_min INT NULL,
    room_t FLOAT NULL,
    room_rh FLOAT NULL,
    divider_ratio FLOAT NULL,
    notes NVARCHAR(1000) NULL,
    qc_flag NVARCHAR(10) NOT NULL DEFAULT 'OK',
    status NVARCHAR(20) NOT NULL,
    dry_run BIT NOT NULL DEFAULT 0,
    error NVARCHAR(500) NULL,
    samples_received INT NOT NULL DEFAULT 0,
    created_at DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME(),
    started_at DATETIME2 NULL,
    finished_at DATETIME2 NULL
)"#,
    r#"IF OBJECT_ID(N'dbo.measurement_samples', N'U') IS NULL
CREATE TABLE dbo.measurement_samples (
    measurement_id NVARCHAR(64) NOT NULL,
    idx INT NOT NULL,
    t_s FLOAT NOT NULL,
    ts NVARCHAR(40) NULL,
    raw1 INT NULL, raw2 INT NULL, raw3 INT NULL, raw4 INT NULL,
    raw5 INT NULL, raw6 INT NULL, raw7 INT NULL, raw8 INT NULL,
    v1 FLOAT NULL, v2 FLOAT NULL, v3 FLOAT NULL, v4 FLOAT NULL,
    v5 FLOAT NULL, v6 FLOAT NULL, v7 FLOAT NULL, v8 FLOAT NULL,
    temp_c FLOAT NULL,
    rh_pct FLOAT NULL,
    dht_age_s INT NULL,
    CONSTRAINT PK_measurement_samples PRIMARY KEY CLUSTERED (measurement_id, idx)
        WITH (IGNORE_DUP_KEY = ON)
)"#,
];

const SESSION_COLS: &str = "s.measurement_id, s.device_id, s.daq_id, s.batch_id, s.group_id, s.coffee_id, \
    s.aliquot_id, s.species, s.bean_state, s.roast_level, s.mass_g, s.operator_name, s.protocol_ver, \
    s.run_order, s.heating_s, s.duration_s, s.sample_period_ms, s.warmup_min, s.room_t, s.room_rh, \
    s.divider_ratio, s.notes, s.qc_flag, s.status, s.dry_run, s.error, s.samples_received, \
    s.created_at, s.started_at, s.finished_at";

impl SqlStore {
    pub fn new(cfg: DbConfig) -> Self {
        Self { cfg, idle: Mutex::new(Vec::new()), limit: Semaphore::new(8) }
    }

    async fn connect(&self) -> Result<Conn> {
        let mut config = Config::new();
        config.host(&self.cfg.host);
        config.port(self.cfg.port);
        config.authentication(AuthMethod::sql_server(&self.cfg.user, &self.cfg.password));
        config.database(&self.cfg.database);
        config.encryption(EncryptionLevel::Required);
        config.trust_cert();
        let tcp = tokio::time::timeout(Duration::from_secs(20), TcpStream::connect(config.get_addr()))
            .await
            .context("Timeout koneksi TCP ke Azure SQL")?
            .context("Gagal koneksi TCP ke Azure SQL (cek firewall Azure: izinkan IP server)")?;
        tcp.set_nodelay(true)?;
        // Azure SQL dapat mengirim redirect ke node lain (port 11000-11999).
        match Client::connect(config.clone(), tcp.compat_write()).await {
            Ok(c) => Ok(c),
            Err(tiberius::error::Error::Routing { host, port }) => {
                let mut config = config;
                config.host(&host);
                config.port(port);
                let tcp = TcpStream::connect(config.get_addr()).await?;
                tcp.set_nodelay(true)?;
                Ok(Client::connect(config, tcp.compat_write()).await?)
            }
            Err(e) => Err(anyhow!("Gagal login Azure SQL: {e}")),
        }
    }

    async fn get(&self) -> Result<Pooled<'_>> {
        let permit = self.limit.acquire().await?;
        let reused = {
            let mut idle = self.idle.lock().unwrap();
            // buang koneksi yang terlalu lama menganggur
            idle.retain(|(_, t)| t.elapsed() < Duration::from_secs(240));
            idle.pop().map(|(c, _)| c)
        };
        let conn = match reused {
            Some(c) => c,
            None => self.connect().await?,
        };
        Ok(Pooled { store: self, conn: Some(conn), _permit: permit })
    }

    async fn migrate(&self) -> Result<()> {
        with_conn!(self, c, {
            for sql in MIGRATIONS {
                c.execute(*sql, &[]).await.with_context(|| format!("Migrasi gagal: {}", &sql[..60.min(sql.len())]))?;
            }
            Ok(())
        })
    }

    async fn ping(&self) -> Result<()> {
        with_conn!(self, c, {
            c.simple_query("SELECT 1").await?.into_results().await?;
            Ok(())
        })
    }

    async fn counts(&self) -> Result<(i64, i64, i64)> {
        with_conn!(self, c, {
            let rows = c
                .simple_query(
                    "SELECT CAST((SELECT COUNT(*) FROM dbo.measurement_sessions) AS BIGINT) AS a, \
                            CAST((SELECT COUNT(*) FROM dbo.sensor_readings) AS BIGINT) AS b, \
                            CAST((SELECT COUNT(*) FROM dbo.measurement_samples) AS BIGINT) AS c",
                )
                .await?
                .into_first_result()
                .await?;
            let r = rows.first().ok_or_else(|| anyhow!("kosong"))?;
            Ok((
                r.try_get::<i64, _>("a")?.unwrap_or(0),
                r.try_get::<i64, _>("b")?.unwrap_or(0),
                r.try_get::<i64, _>("c")?.unwrap_or(0),
            ))
        })
    }

    async fn insert_session(&self, s: &Session) -> Result<()> {
        with_conn!(self, c, {
            let mut q = Query::new(
                "INSERT INTO dbo.measurement_sessions (measurement_id, device_id, daq_id, batch_id, group_id, \
                 coffee_id, aliquot_id, species, bean_state, roast_level, mass_g, operator_name, protocol_ver, \
                 run_order, heating_s, duration_s, sample_period_ms, warmup_min, room_t, room_rh, divider_ratio, \
                 notes, qc_flag, status, dry_run) VALUES (@P1, @P2, @P3, @P4, @P5, @P6, @P7, @P8, @P9, @P10, \
                 @P11, @P12, @P13, @P14, @P15, @P16, @P17, @P18, @P19, @P20, @P21, @P22, @P23, @P24, @P25)",
            );
            q.bind(s.measurement_id.clone());
            q.bind(s.device_id.clone());
            q.bind(s.daq_id.clone());
            q.bind(s.batch_id.clone());
            q.bind(s.group_id.clone());
            q.bind(s.coffee_id.clone());
            q.bind(s.aliquot_id.clone());
            q.bind(s.species.clone());
            q.bind(s.bean_state.clone());
            q.bind(s.roast_level.clone());
            q.bind(s.mass_g);
            q.bind(s.operator_name.clone());
            q.bind(s.protocol_ver.clone());
            q.bind(s.run_order);
            q.bind(s.heating_s);
            q.bind(s.duration_s);
            q.bind(s.sample_period_ms);
            q.bind(s.warmup_min);
            q.bind(s.room_t);
            q.bind(s.room_rh);
            q.bind(s.divider_ratio);
            q.bind(s.notes.clone());
            q.bind(s.qc_flag.clone());
            q.bind(s.status.clone());
            q.bind(s.dry_run);
            q.execute(c).await.context("Gagal menyimpan sesi (Measurement_ID mungkin sudah ada)")?;
            Ok(())
        })
    }

    async fn set_status(&self, id: &str, change: StatusChange, error: Option<&str>) -> Result<()> {
        let from = change.allowed_from().iter().map(|s| format!("'{s}'")).collect::<Vec<_>>().join(",");
        let time_col = if change == StatusChange::Recording {
            "started_at = COALESCE(started_at, SYSUTCDATETIME())"
        } else {
            "finished_at = SYSUTCDATETIME()"
        };
        let sql = format!(
            "UPDATE dbo.measurement_sessions SET status = @P2, error = COALESCE(@P3, error), {time_col} \
             WHERE measurement_id = @P1 AND status IN ({from})"
        );
        with_conn!(self, c, {
            let mut q = Query::new(sql.as_str());
            q.bind(id.to_string());
            q.bind(change.as_str());
            q.bind(error.map(|e| e.chars().take(500).collect::<String>()));
            q.execute(c).await?;
            Ok(())
        })
    }

    async fn set_qc(&self, id: &str, qc: &str, notes: Option<&str>) -> Result<bool> {
        with_conn!(self, c, {
            let mut q = Query::new(
                "UPDATE dbo.measurement_sessions SET qc_flag = @P2, notes = COALESCE(@P3, notes) WHERE measurement_id = @P1",
            );
            q.bind(id.to_string());
            q.bind(qc.to_string());
            q.bind(notes.map(|n| n.chars().take(1000).collect::<String>()));
            let r = q.execute(c).await?;
            Ok(r.total() > 0)
        })
    }

    async fn insert_samples(&self, id: &str, rows: &[SampleRow]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        with_conn!(self, c, {
            const COLS: usize = 24;
            for chunk in rows.chunks(40) {
                let mut sql = String::from(
                    "INSERT INTO dbo.measurement_samples (measurement_id, idx, t_s, ts, raw1, raw2, raw3, raw4, raw5, \
                     raw6, raw7, raw8, v1, v2, v3, v4, v5, v6, v7, v8, temp_c, rh_pct, dht_age_s) VALUES ",
                );
                // @P1 = measurement_id dipakai bersama; tiap baris 23 parameter lain.
                let mut p = 2;
                for (n, _) in chunk.iter().enumerate() {
                    if n > 0 {
                        sql.push(',');
                    }
                    let ph: Vec<String> = (0..COLS - 2).map(|k| format!("@P{}", p + k)).collect();
                    sql.push_str(&format!("(@P1,{})", ph.join(",")));
                    p += COLS - 2;
                }
                let mut q = Query::new(sql);
                q.bind(id.to_string());
                for r in chunk {
                    q.bind(r.i);
                    q.bind(r.t_s);
                    q.bind(r.ts.clone());
                    for k in 0..SENSOR_COUNT {
                        q.bind(r.raw.get(k).copied().flatten());
                    }
                    for k in 0..SENSOR_COUNT {
                        q.bind(r.v.get(k).copied().flatten().filter(|x| x.is_finite()));
                    }
                    q.bind(r.temp_c.filter(|x| x.is_finite()));
                    q.bind(r.rh_pct.filter(|x| x.is_finite()));
                    q.bind(r.dht_age_s);
                }
                q.execute(&mut *c).await.context("Gagal menyimpan data mentah")?;
            }
            let mut q = Query::new(
                "UPDATE dbo.measurement_sessions SET samples_received = \
                 (SELECT COUNT(*) FROM dbo.measurement_samples WHERE measurement_id = @P1) WHERE measurement_id = @P1",
            );
            q.bind(id.to_string());
            q.execute(c).await?;
            Ok(())
        })
    }

    async fn insert_reading(&self, r: &Reading) -> Result<bool> {
        let late = serde_json::to_string(&r.sensor_late_mean)?;
        let peak = serde_json::to_string(&r.sensor_peak_abs)?;
        with_conn!(self, c, {
            let mut q = Query::new(
                "IF EXISTS (SELECT 1 FROM dbo.sensor_readings WHERE measurement_id = @P3) SELECT CAST(0 AS INT) AS inserted \
                 ELSE BEGIN \
                 INSERT INTO dbo.sensor_readings (device_id, fw_version, measurement_id, daq_id, batch_id, group_id, \
                 coffee_id, aliquot_id, species, bean_state, predicted_class, confidence, model_version, temperature, \
                 humidity, sample_count, sample_duration_s, inference_time_ms, sensor_late_mean, sensor_peak_abs, seq, \
                 reading_timestamp) VALUES (@P1, @P2, @P3, @P4, @P5, @P6, @P7, @P8, @P9, @P10, @P11, @P12, @P13, @P14, \
                 @P15, @P16, @P17, @P18, @P19, @P20, @P21, @P22); SELECT CAST(1 AS INT) AS inserted END",
            );
            q.bind(r.device_id.clone());
            q.bind(r.fw_version.clone());
            q.bind(r.measurement_id.clone());
            q.bind(r.daq_id.clone());
            q.bind(r.batch_id.clone());
            q.bind(r.group_id.clone());
            q.bind(r.coffee_id.clone());
            q.bind(r.aliquot_id.clone());
            q.bind(r.species.clone());
            q.bind(r.bean_state.clone());
            q.bind(r.predicted_class.clone());
            q.bind(r.confidence.filter(|x| x.is_finite()));
            q.bind(r.model_version.clone());
            q.bind(r.temperature.filter(|x| x.is_finite()));
            q.bind(r.humidity.filter(|x| x.is_finite()));
            q.bind(r.sample_count);
            q.bind(r.sample_duration_s);
            q.bind(r.inference_time_ms);
            q.bind(late);
            q.bind(peak);
            q.bind(r.seq);
            q.bind(r.timestamp.clone());
            let rows = q.query(&mut *c).await?.into_first_result().await?;
            let inserted = rows.first().and_then(|row| row.try_get::<i32, _>("inserted").ok().flatten()).unwrap_or(0) == 1;

            // Pastikan ada baris sesi (pengukuran lama via serial tidak punya sesi).
            let s = session_from_reading(r);
            let mut q = Query::new(
                "IF NOT EXISTS (SELECT 1 FROM dbo.measurement_sessions WHERE measurement_id = @P1) \
                 INSERT INTO dbo.measurement_sessions (measurement_id, device_id, daq_id, batch_id, group_id, coffee_id, \
                 aliquot_id, species, bean_state, duration_s, sample_period_ms, qc_flag, status, notes, finished_at) \
                 VALUES (@P1, @P2, @P3, @P4, @P5, @P6, @P7, @P8, @P9, @P10, @P11, 'OK', 'completed', @P12, SYSUTCDATETIME())",
            );
            q.bind(s.measurement_id);
            q.bind(s.device_id);
            q.bind(s.daq_id);
            q.bind(s.batch_id);
            q.bind(s.group_id);
            q.bind(s.coffee_id);
            q.bind(s.aliquot_id);
            q.bind(s.species);
            q.bind(s.bean_state);
            q.bind(s.duration_s);
            q.bind(s.sample_period_ms);
            q.bind(s.notes);
            q.execute(c).await?;
            Ok(inserted)
        })
    }

    async fn list(&self) -> Result<Vec<MeasurementRow>> {
        let sql = format!(
            "SELECT {SESSION_COLS}, r.predicted_class, r.confidence, r.model_version, r.fw_version, r.temperature, \
             r.humidity, r.sample_count, r.sample_duration_s, r.inference_time_ms, r.sensor_late_mean, \
             r.sensor_peak_abs, r.reading_timestamp, CAST(CASE WHEN r.measurement_id IS NULL THEN 0 ELSE 1 END AS BIT) AS has_result \
             FROM dbo.measurement_sessions s \
             OUTER APPLY (SELECT TOP 1 * FROM dbo.sensor_readings x WHERE x.measurement_id = s.measurement_id ORDER BY x.id DESC) r \
             ORDER BY s.created_at DESC"
        );
        with_conn!(self, c, {
            let rows = c.simple_query(sql.as_str()).await?.into_first_result().await?;
            Ok(rows.iter().map(row_to_measurement).collect())
        })
    }

    async fn samples(&self, id: &str) -> Result<Vec<SampleRow>> {
        with_conn!(self, c, {
            let mut q = Query::new(
                "SELECT idx, t_s, ts, raw1, raw2, raw3, raw4, raw5, raw6, raw7, raw8, v1, v2, v3, v4, v5, v6, v7, v8, \
                 temp_c, rh_pct, dht_age_s FROM dbo.measurement_samples WHERE measurement_id = @P1 ORDER BY idx",
            );
            q.bind(id.to_string());
            let rows = q.query(c).await?.into_first_result().await?;
            Ok(rows
                .iter()
                .map(|r| SampleRow {
                    i: gi(r, "idx").unwrap_or(0),
                    t_s: gf(r, "t_s").unwrap_or(0.0),
                    ts: gs(r, "ts"),
                    raw: (1..=SENSOR_COUNT).map(|k| gi(r, format!("raw{k}").as_str())).collect(),
                    v: (1..=SENSOR_COUNT).map(|k| gf(r, format!("v{k}").as_str())).collect(),
                    temp_c: gf(r, "temp_c"),
                    rh_pct: gf(r, "rh_pct"),
                    dht_age_s: gi(r, "dht_age_s"),
                })
                .collect())
        })
    }
}

fn gs(r: &Row, c: &str) -> Option<String> {
    r.try_get::<&str, _>(c).ok().flatten().map(str::to_string)
}
fn gf(r: &Row, c: &str) -> Option<f64> {
    r.try_get::<f64, _>(c).ok().flatten()
}
fn gi(r: &Row, c: &str) -> Option<i32> {
    r.try_get::<i32, _>(c).ok().flatten()
}
fn gb(r: &Row, c: &str) -> Option<bool> {
    r.try_get::<bool, _>(c).ok().flatten()
}
fn gdt(r: &Row, c: &str) -> Option<DateTime<Utc>> {
    r.try_get::<NaiveDateTime, _>(c).ok().flatten().map(|d| d.and_utc())
}
fn gjson(r: &Row, c: &str) -> Vec<Option<f64>> {
    gs(r, c).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn row_to_measurement(r: &Row) -> MeasurementRow {
    let session = Session {
        measurement_id: gs(r, "measurement_id").unwrap_or_default(),
        device_id: gs(r, "device_id").unwrap_or_default(),
        daq_id: gs(r, "daq_id"),
        batch_id: gs(r, "batch_id"),
        group_id: gs(r, "group_id"),
        coffee_id: gs(r, "coffee_id"),
        aliquot_id: gs(r, "aliquot_id"),
        species: gs(r, "species"),
        bean_state: gs(r, "bean_state"),
        roast_level: gs(r, "roast_level"),
        mass_g: gf(r, "mass_g"),
        operator_name: gs(r, "operator_name"),
        protocol_ver: gs(r, "protocol_ver"),
        run_order: gi(r, "run_order"),
        heating_s: gi(r, "heating_s"),
        duration_s: gi(r, "duration_s").unwrap_or(500),
        sample_period_ms: gi(r, "sample_period_ms").unwrap_or(1000),
        warmup_min: gi(r, "warmup_min"),
        room_t: gf(r, "room_t"),
        room_rh: gf(r, "room_rh"),
        divider_ratio: gf(r, "divider_ratio"),
        notes: gs(r, "notes"),
        qc_flag: gs(r, "qc_flag").unwrap_or_else(|| "OK".into()),
        status: gs(r, "status").unwrap_or_default(),
        dry_run: gb(r, "dry_run").unwrap_or(false),
        error: gs(r, "error"),
        samples_received: gi(r, "samples_received").unwrap_or(0),
        created_at: gdt(r, "created_at"),
        started_at: gdt(r, "started_at"),
        finished_at: gdt(r, "finished_at"),
    };
    let result = if gb(r, "has_result").unwrap_or(false) {
        Some(ResultView {
            predicted_class: gs(r, "predicted_class"),
            confidence: gf(r, "confidence"),
            model_version: gs(r, "model_version"),
            fw_version: gs(r, "fw_version"),
            temperature: gf(r, "temperature"),
            humidity: gf(r, "humidity"),
            sample_count: gi(r, "sample_count"),
            sample_duration_s: gf(r, "sample_duration_s"),
            inference_time_ms: gf(r, "inference_time_ms"),
            sensor_late_mean: gjson(r, "sensor_late_mean"),
            sensor_peak_abs: gjson(r, "sensor_peak_abs"),
            reading_timestamp: gs(r, "reading_timestamp"),
        })
    } else {
        None
    };
    MeasurementRow { session, result }
}
