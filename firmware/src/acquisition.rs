//! Thread akuisisi (prioritas utama): warm-up → idle (preview live) → rekam.
//! Tidak pernah memanggil jaringan; data dikirim lewat channel ke thread jaringan,
//! sehingga gangguan WiFi tidak menimbulkan missing sample/jitter (handbook 10.5).

use crate::ads1115;
use crate::config::*;
use crate::dht22::Dht22;
use crate::ei_model::EiModel;
use crate::features::FeatureAccumulator;
use crate::shared::*;
use esp_idf_hal::i2c::I2cDriver;
use log::*;
use serde::Serialize;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DHT_PERIOD: Duration = Duration::from_secs(2);

pub struct AcqCtx {
    pub i2c: I2cDriver<'static>,
    pub dht: Option<Dht22<'static>>,
    pub model: EiModel,
    pub shared: SharedRef,
    pub rx: Receiver<AcqCommand>,
    pub tx: SyncSender<Outgoing>,
    pub abort: Arc<AtomicBool>,
}

/// Payload hasil akhir — format sama dengan firmware versi sebelumnya (kompatibel backend).
#[derive(Serialize)]
struct PredictionPayload<'a> {
    device_id: &'a str,
    fw_version: &'a str,
    measurement_id: &'a str,
    daq_id: &'a str,
    batch_id: &'a str,
    group_id: &'a str,
    coffee_id: &'a str,
    aliquot_id: &'a str,
    species: &'a str,
    bean_state: &'a str,
    predicted_class: String,
    confidence: f32,
    model_version: String,
    temperature: Option<f32>,
    humidity: Option<f32>,
    sample_count: usize,
    sample_duration_s: f32,
    inference_time_ms: f32,
    sensor_late_mean: [f32; 8],
    sensor_peak_abs: [f32; 8],
    seq: u64,
    timestamp: String,
}

struct Env {
    last: Option<(f32, f32)>,
    last_ok: Option<Instant>,
    last_try: Instant,
}

impl AcqCtx {
    fn event(&self, measurement_id: Option<&str>, event: &str, message: Option<&str>, values: Option<Vec<f32>>) {
        let body = json!({ "device_id": DEVICE_ID, "measurement_id": measurement_id, "event": event,
                           "message": message, "values": values }).to_string();
        let _ = self.tx.send(Outgoing::Post { path: "/api/device/event", body });
    }

    /// Baca DHT22 bila sudah ≥2 s sejak percobaan terakhir; nilai lama di-hold (zero-order hold).
    fn read_env(&mut self, env: &mut Env) {
        if env.last_try.elapsed() < DHT_PERIOD {
            return;
        }
        env.last_try = Instant::now();
        if let Some(d) = self.dht.as_mut() {
            match d.read() {
                Ok(v) => {
                    env.last = Some(v);
                    env.last_ok = Some(Instant::now());
                }
                Err(e) => debug!("DHT22: {e:#}"),
            }
        }
    }

    fn update_live(&mut self, r: &ads1115::Reading, env: &Env) {
        let mut s = self.shared.lock().unwrap();
        s.sensors.ads_a = r.chip_a_ok;
        s.sensors.ads_b = r.chip_b_ok;
        s.sensors.dht = env.last_ok.map(|t| t.elapsed() < Duration::from_secs(10)).unwrap_or(false);
        s.live = Some(LiveReading {
            v: r.raw.map(|x| x.map(ads1115::to_voltage)),
            temp_c: env.last.map(|e| e.0),
            rh_pct: env.last.map(|e| e.1),
        });
    }
}

pub fn run(mut ctx: AcqCtx) {
    let boot = Instant::now();
    let mut env = Env { last: None, last_ok: None, last_try: Instant::now() - DHT_PERIOD };
    let mut seq = 0u64;
    loop {
        let left = WARMUP_SECONDS.saturating_sub(boot.elapsed().as_secs());
        {
            let mut s = ctx.shared.lock().unwrap();
            if s.state != DevState::Ota {
                s.state = if left > 0 { DevState::WarmingUp } else { DevState::Idle };
            }
            s.warmup_remaining_s = left as u32;
        }

        // Preview live setiap ±2 detik (untuk kartu sensor di dashboard)
        let r = ads1115::read_all(&mut ctx.i2c);
        ctx.read_env(&mut env);
        ctx.update_live(&r, &env);

        match ctx.rx.recv_timeout(Duration::from_secs(2)) {
            Ok(AcqCommand::Start { meta, duration_s, period_ms, allow_missing }) => {
                if ctx.shared.lock().unwrap().state == DevState::Ota {
                    ctx.event(Some(&meta.measurement_id), "failed", Some("Perangkat sedang update firmware"), None);
                } else if left > 0 {
                    ctx.event(Some(&meta.measurement_id), "failed", Some(&format!("Sensor masih warm-up ({left} detik lagi)")), None);
                } else {
                    seq += 1;
                    record(&mut ctx, &mut env, &meta, duration_s, period_ms.clamp(500, 5000), allow_missing, seq);
                }
            }
            Ok(AcqCommand::ZeroBaseline) => zero_baseline(&mut ctx, &mut env),
            Ok(AcqCommand::SetPeriod(p)) => {
                ctx.shared.lock().unwrap().sample_period_ms = p.clamp(500, 5000);
                info!("Interval sampling default = {p} ms");
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                error!("Channel perintah terputus");
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    }
}

fn zero_baseline(ctx: &mut AcqCtx, env: &mut Env) {
    info!("Merekam zero baseline 10 detik…");
    let mut sum = [0f64; 8];
    let mut n = [0u32; 8];
    for _ in 0..10 {
        let r = ads1115::read_all(&mut ctx.i2c);
        for k in 0..8 {
            if let Some(x) = r.raw[k] {
                sum[k] += ads1115::to_voltage(x) as f64;
                n[k] += 1;
            }
        }
        ctx.read_env(env);
        ctx.update_live(&r, env);
        std::thread::sleep(Duration::from_secs(1));
    }
    let vals: Vec<f32> = (0..8).map(|k| if n[k] > 0 { (sum[k] / n[k] as f64) as f32 } else { f32::NAN }).collect();
    ctx.event(None, "zero_baseline", Some("Baseline udara bersih direkam (rata-rata 10 s)"), Some(vals));
}

fn record(ctx: &mut AcqCtx, env: &mut Env, meta: &MeasurementMeta, duration_s: u32, period_ms: u32, allow_missing: bool, seq: u64) {
    let mid = meta.measurement_id.as_str();
    let total = (duration_s as u64 * 1000 / period_ms as u64) as usize;
    let period = Duration::from_millis(period_ms as u64);

    // Cek sensor sebelum mulai
    let probe_a = ads1115::probe(&mut ctx.i2c, ads1115::ADDR_A);
    let probe_b = ads1115::probe(&mut ctx.i2c, ads1115::ADDR_B);
    if !(probe_a && probe_b) && !allow_missing {
        let msg = format!(
            "ADS1115 tidak terdeteksi:{}{}",
            if probe_a { "" } else { " 0x48" },
            if probe_b { "" } else { " 0x49" }
        );
        ctx.shared.lock().unwrap().last_error = Some(msg.clone());
        ctx.event(Some(mid), "failed", Some(&msg), None);
        return;
    }

    info!("MEASUREMENT_START,{mid},{total} sampel @ {period_ms} ms");
    {
        let mut s = ctx.shared.lock().unwrap();
        s.state = DevState::Recording;
        s.measurement_id = Some(mid.to_string());
        s.samples_done = 0;
        s.samples_total = total as u32;
        s.last_error = None;
    }
    ctx.event(Some(mid), "started", None, None);
    info!("RAW_CSV_HEADER,t_s,timestamp,mq3_raw,mq6_raw,mq7_raw,mq135_raw,tgs2600_raw,tgs2602_raw,tgs2611_raw,tgs2620_raw,mq3_v,mq6_v,mq7_v,mq135_v,tgs2600_v,tgs2602_v,tgs2611_v,tgs2620_v,temp_c,rh_pct,dht_age_s");

    let started = Instant::now();
    let first_ts = utc_now();
    let mut acc = FeatureAccumulator::new();
    let mut volts: Vec<[f32; 8]> = Vec::with_capacity(total);
    let (mut t_sum, mut h_sum, mut env_n) = (0f64, 0f64, 0u32);
    let mut jitter_bad = 0u32;
    let mut prev_t = 0f32;
    let mut done = 0usize;

    for idx in 0..total {
        // Timer monotonik (bukan delay berantai) → jitter tidak menumpuk
        let deadline = started + period * idx as u32;
        if let Some(w) = deadline.checked_duration_since(Instant::now()) {
            std::thread::sleep(w);
        }
        if ctx.abort.swap(false, Ordering::SeqCst) {
            info!("MEASUREMENT_CANCELLED,{mid},{idx}");
            reset_state(ctx);
            ctx.event(Some(mid), "cancelled", Some(&format!("Dihentikan operator pada sampel {idx}")), None);
            return;
        }
        let t_s = started.elapsed().as_secs_f32();
        if idx > 0 && ((t_s - prev_t) * 1000.0 - period_ms as f32).abs() > 50.0 {
            jitter_bad += 1;
        }
        prev_t = t_s;

        let r = ads1115::read_all(&mut ctx.i2c);
        ctx.read_env(env);
        let raw_u16: [u16; 8] = r.raw.map(|x| x.unwrap_or(0));
        let _ = acc.push(raw_u16, t_s);
        let v: [Option<f32>; 8] = r.raw.map(|x| x.map(ads1115::to_voltage));
        volts.push(v.map(|x| x.unwrap_or(0.0)));
        if let Some((t, h)) = env.last {
            t_sum += t as f64;
            h_sum += h as f64;
            env_n += 1;
        }
        let row = SampleRow {
            i: idx as u32,
            t_s,
            ts: utc_now(),
            raw: r.raw,
            v,
            temp_c: env.last.map(|e| e.0),
            rh_pct: env.last.map(|e| e.1),
            dht_age_s: env.last_ok.map(|t| t.elapsed().as_secs() as u32),
        };
        log_csv(&row);
        ctx.update_live(&r, env);
        if ctx.tx.try_send(Outgoing::Sample { measurement_id: mid.to_string(), row }).is_err() {
            warn!("Antrian unggah penuh — sampel {idx} hanya ada di log serial");
        }
        done = idx + 1;
        ctx.shared.lock().unwrap().samples_done = done as u32;
        if idx % 50 == 0 {
            info!("ACQUISITION,{}/{total}", idx + 1);
        }
    }

    // Fitur + inferensi TinyML
    ctx.shared.lock().unwrap().state = DevState::Uploading;
    let (predicted_class, confidence, inference_time_ms, model_version) = match ctx.model.predict(&volts) {
        Some(p) => {
            info!("PREDICTION,{},{:.3}", p.label, p.confidence);
            (p.label, p.confidence, p.inference_time_ms, p.model_version)
        }
        None => ("CAPTURE_ONLY".to_string(), 0.0, 0.0, "none".to_string()),
    };
    let payload = PredictionPayload {
        device_id: DEVICE_ID,
        fw_version: FW_VERSION,
        measurement_id: mid,
        daq_id: &meta.daq_id,
        batch_id: &meta.batch_id,
        group_id: &meta.group_id,
        coffee_id: &meta.coffee_id,
        aliquot_id: &meta.aliquot_id,
        species: &meta.species,
        bean_state: &meta.bean_state,
        predicted_class,
        confidence,
        model_version,
        temperature: (env_n > 0).then(|| (t_sum / env_n as f64) as f32),
        humidity: (env_n > 0).then(|| (h_sum / env_n as f64) as f32),
        sample_count: done,
        sample_duration_s: started.elapsed().as_secs_f32(),
        inference_time_ms,
        sensor_late_mean: acc.late_mean(),
        sensor_peak_abs: acc.peak_abs(),
        seq,
        timestamp: first_ts,
    };
    let body = serde_json::to_string(&payload).unwrap_or_default();
    info!("PREDICTION_PAYLOAD,{body}");
    let _ = ctx.tx.send(Outgoing::Post { path: "/api/readings", body });
    let note = if jitter_bad > 0 { Some(format!("{jitter_bad} sampel di luar toleransi jitter ±50 ms")) } else { None };
    ctx.event(Some(mid), "completed", note.as_deref(), None);
    info!("MEASUREMENT_COMPLETE,{mid},{done} rows");
    reset_state(ctx);
}

fn reset_state(ctx: &AcqCtx) {
    let mut s = ctx.shared.lock().unwrap();
    s.state = DevState::Idle;
    s.measurement_id = None;
    s.samples_done = 0;
    s.samples_total = 0;
}

/// Tetap mencetak CSV ke serial — jalur cadangan bila internet putus.
fn log_csv(r: &SampleRow) {
    let o = |x: Option<f32>, d: usize| x.map(|v| format!("{v:.d$}")).unwrap_or_default();
    let raw: Vec<String> = r.raw.iter().map(|x| x.map(|v| v.to_string()).unwrap_or_default()).collect();
    let v: Vec<String> = r.v.iter().map(|x| o(*x, 6)).collect();
    info!(
        "RAW_CSV,{:.3},{},{},{},{},{},{}",
        r.t_s,
        r.ts,
        raw.join(","),
        v.join(","),
        o(r.temp_c, 1),
        o(r.rh_pct, 1),
        r.dht_age_s.map(|x| x.to_string()).unwrap_or_default()
    );
}

pub fn utc_now() -> String {
    let epoch = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let days = epoch.div_euclid(86_400);
    let secs = epoch.rem_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", secs / 3600, secs % 3600 / 60, secs % 60)
}
