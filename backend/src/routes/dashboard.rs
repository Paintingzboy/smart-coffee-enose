//! Endpoint untuk dashboard web: baca data (publik) dan kontrol (butuh kunci operator).

use super::require_operator;
use crate::error::{ApiError, ApiResult};
use crate::models::*;
use crate::state::{ActiveSession, Shared};
use crate::store::StatusChange;
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

// ───────────────────────────── Status sistem ─────────────────────────────

pub async fn status(State(st): State<Shared>) -> ApiResult<Json<Value>> {
    let t0 = Instant::now();
    let infra = {
        let cached = st.health_cache.read().await.clone();
        match cached {
            Some((t, v)) if t.elapsed() < Duration::from_secs(15) => v,
            _ => {
                let db = match st.store.ping().await {
                    Ok(lat) => {
                        let counts = st.store.counts().await.ok();
                        json!({
                            "kind": st.store.kind(), "ok": true,
                            "latency_ms": lat.as_secs_f64() * 1000.0,
                            "sessions": counts.map(|c| c.0), "readings": counts.map(|c| c.1), "samples": counts.map(|c| c.2),
                        })
                    }
                    Err(e) => json!({ "kind": st.store.kind(), "ok": false, "error": format!("{e:#}") }),
                };
                let tb = match &st.tb {
                    None => json!({ "configured": false, "ok": false }),
                    Some(tb) => {
                        let t = Instant::now();
                        match tb.check().await {
                            Ok(_) => json!({ "configured": true, "ok": true, "url": tb.url(), "latency_ms": t.elapsed().as_secs_f64() * 1000.0 }),
                            Err(e) => json!({ "configured": true, "ok": false, "url": tb.url(), "error": format!("{e:#}") }),
                        }
                    }
                };
                let v = json!({ "database": db, "thingsboard": tb });
                *st.health_cache.write().await = Some((Instant::now(), v.clone()));
                v
            }
        }
    };
    let devices: Vec<_> = st.devices.read().await.values().map(|d| d.summary()).collect();
    Ok(Json(json!({
        "backend": {
            "ok": true,
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_s": st.started.elapsed().as_secs(),
            "latency_ms": t0.elapsed().as_secs_f64() * 1000.0,
            "server_time": Utc::now().to_rfc3339(),
        },
        "database": infra["database"],
        "thingsboard": infra["thingsboard"],
        "devices": devices,
        "model": st.cfg.model,
        "fw_title": st.cfg.fw_title,
    })))
}

pub async fn model(State(st): State<Shared>) -> Json<Value> {
    Json(json!(st.cfg.model))
}

pub async fn auth_check(State(st): State<Shared>, h: HeaderMap) -> ApiResult<Json<Value>> {
    require_operator(&h, &st)?;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Perangkat ─────────────────────────────

pub async fn devices(State(st): State<Shared>) -> Json<Value> {
    let mut list: Vec<_> = st.devices.read().await.values().map(|d| d.summary()).collect();
    list.sort_by(|a, b| a.device_id.cmp(&b.device_id));
    Json(json!(list))
}

#[derive(Deserialize)]
pub struct LiveQuery {
    /// hanya kirim sampel dengan indeks > since (polling inkremental)
    since: Option<i32>,
}

pub async fn device_live(State(st): State<Shared>, Path(id): Path<String>, Query(q): Query<LiveQuery>) -> ApiResult<Json<Value>> {
    let devices = st.devices.read().await;
    let dev = devices.get(&id).ok_or_else(|| ApiError::not_found(format!("Perangkat {id} belum pernah terhubung")))?;
    let since = q.since.unwrap_or(-1);
    let samples: Vec<&SampleRow> = dev.samples.iter().filter(|s| s.i > since).collect();
    let history: Vec<_> = dev.live_history.iter().rev().take(120).rev().collect();
    Ok(Json(json!({
        "device": dev.summary(),
        "samples_measurement": dev.samples_measurement,
        "samples": samples,
        "live_history": history,
        "events": dev.events,
    })))
}

#[derive(Deserialize)]
pub struct CommandBody {
    #[serde(rename = "type")]
    kind: String,
    sample_period_ms: Option<u32>,
}

/// Perintah sederhana: stop | zero_baseline | set_config | check_ota | reboot.
pub async fn device_command(
    State(st): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<CommandBody>,
) -> ApiResult<Json<Value>> {
    require_operator(&h, &st)?;
    if !["stop", "zero_baseline", "set_config", "check_ota", "reboot"].contains(&body.kind.as_str()) {
        return Err(ApiError::bad_request("Gunakan endpoint /api/measurements/start untuk mulai rekam"));
    }
    if let Some(p) = body.sample_period_ms {
        if !(500..=5000).contains(&p) {
            return Err(ApiError::bad_request("Sampling interval harus 500–5000 ms"));
        }
    }
    let cmd = Command {
        id: st.next_command_id(),
        kind: body.kind.clone(),
        measurement: None,
        duration_s: None,
        sample_period_ms: body.sample_period_ms,
        allow_missing_sensors: None,
        issued_at: Utc::now(),
    };
    let mut devices = st.devices.write().await;
    let dev = devices.get_mut(&id).ok_or_else(|| ApiError::not_found("Perangkat tidak ditemukan"))?;
    if !dev.online() {
        return Err(ApiError::conflict("Perangkat offline"));
    }
    if body.kind != "stop" && dev.hb.state == "recording" {
        return Err(ApiError::conflict("Perangkat sedang merekam — hentikan dulu"));
    }
    dev.log("command", None, Some(&format!("Perintah '{}' dikirim", body.kind)));
    dev.pending_command = Some(cmd.clone());
    Ok(Json(json!({ "ok": true, "command": cmd })))
}

// ───────────────────────────── Mulai pengukuran ─────────────────────────────

#[derive(Deserialize)]
pub struct StartBody {
    device_id: String,
    measurement_id: Option<String>,
    daq_id: String,
    batch_id: String,
    group_id: String,
    coffee_id: String,
    aliquot_id: String,
    species: String,
    bean_state: String,
    roast_level: Option<String>,
    mass_g: Option<f64>,
    operator_name: Option<String>,
    protocol_ver: Option<String>,
    run_order: Option<i32>,
    heating_s: Option<i32>,
    warmup_min: Option<i32>,
    room_t: Option<f64>,
    room_rh: Option<f64>,
    divider_ratio: Option<f64>,
    notes: Option<String>,
    duration_s: Option<u32>,
    sample_period_ms: Option<u32>,
    #[serde(default)]
    dry_run: bool,
}

fn clean_id(v: &str, field: &str) -> Result<String, ApiError> {
    let v = v.trim();
    if v.is_empty() || v.len() > 64 || !v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(ApiError::bad_request(format!("{field} wajib diisi (huruf, angka, garis bawah)")));
    }
    Ok(v.to_string())
}

fn digits(s: &str) -> u32 {
    s.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

/// Measurement_ID baku handbook: DAQ03_B02_C07_A04_M04.
fn build_measurement_id(daq: &str, batch: &str, coffee: &str, aliquot: &str, m: usize) -> String {
    format!("DAQ{:02}_B{:02}_{}_{}_M{:02}", digits(daq), digits(batch), coffee, aliquot, m)
}

fn next_m(rows: &[MeasurementRow], coffee: &str) -> usize {
    rows.iter().filter(|r| r.session.coffee_id.as_deref() == Some(coffee)).count() + 1
}

#[derive(Deserialize)]
pub struct NextIdQuery {
    daq_id: String,
    batch_id: String,
    coffee_id: String,
    aliquot_id: String,
}

pub async fn next_id(State(st): State<Shared>, Query(q): Query<NextIdQuery>) -> ApiResult<Json<Value>> {
    let rows = st.measurements().await?;
    let m = next_m(&rows, &q.coffee_id);
    Ok(Json(json!({ "measurement_id": build_measurement_id(&q.daq_id, &q.batch_id, &q.coffee_id, &q.aliquot_id, m), "m": m })))
}

pub async fn start_measurement(State(st): State<Shared>, h: HeaderMap, Json(b): Json<StartBody>) -> ApiResult<Json<Value>> {
    require_operator(&h, &st)?;
    let daq_id = clean_id(&b.daq_id, "DAQ_ID")?;
    let batch_id = clean_id(&b.batch_id, "Batch_ID")?;
    let group_id = clean_id(&b.group_id, "Group_ID")?;
    let coffee_id = clean_id(&b.coffee_id, "Coffee_ID")?;
    let aliquot_id = clean_id(&b.aliquot_id, "Aliquot_ID")?;
    let species = clean_id(&b.species, "Species")?;
    let bean_state = clean_id(&b.bean_state, "Bean_State")?;
    let duration_s = b.duration_s.unwrap_or(500);
    let period = b.sample_period_ms.unwrap_or(1000);
    if !(10..=1800).contains(&duration_s) {
        return Err(ApiError::bad_request("Durasi rekaman 10–1800 detik"));
    }
    if !(500..=5000).contains(&period) {
        return Err(ApiError::bad_request("Sampling interval 500–5000 ms"));
    }
    if let Some(m) = b.mass_g {
        if !(0.0..=1000.0).contains(&m) {
            return Err(ApiError::bad_request("Massa tidak wajar"));
        }
    }

    // Validasi perangkat
    {
        let devices = st.devices.read().await;
        let dev = devices.get(&b.device_id).ok_or_else(|| ApiError::not_found("Perangkat belum terhubung ke server"))?;
        if !dev.online() {
            return Err(ApiError::conflict("Perangkat offline — pastikan ESP32 menyala dan terhubung WiFi"));
        }
        match dev.hb.state.as_str() {
            "idle" => {}
            "warming_up" => {
                return Err(ApiError::conflict(format!(
                    "Sensor masih warm-up ({} detik lagi)",
                    dev.hb.warmup_remaining_s.unwrap_or(0)
                )))
            }
            s => return Err(ApiError::conflict(format!("Perangkat sedang sibuk ({s})"))),
        }
        if dev.pending_command.is_some() {
            return Err(ApiError::conflict("Masih ada perintah yang belum diterima perangkat"));
        }
        if !b.dry_run && !(dev.hb.sensors.ads_a && dev.hb.sensors.ads_b) {
            return Err(ApiError::conflict(
                "ADS1115 belum terdeteksi. Pasang sensor, atau aktifkan 'Mode uji tanpa sensor' untuk menguji alur data.",
            ));
        }
    }

    let rows = st.measurements().await?;
    let measurement_id = match b.measurement_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => clean_id(id, "Measurement_ID")?,
        None => build_measurement_id(&daq_id, &batch_id, &coffee_id, &aliquot_id, next_m(&rows, &coffee_id)),
    };
    if rows.iter().any(|r| r.session.measurement_id == measurement_id) {
        return Err(ApiError::conflict(format!("Measurement_ID {measurement_id} sudah dipakai")));
    }

    let mut notes = b.notes.clone().filter(|n| !n.trim().is_empty());
    if b.dry_run {
        notes = Some(format!("[MODE UJI TANPA SENSOR] {}", notes.unwrap_or_default()).trim().to_string());
    }
    let session = Session {
        measurement_id: measurement_id.clone(),
        device_id: b.device_id.clone(),
        daq_id: Some(daq_id.clone()),
        batch_id: Some(batch_id.clone()),
        group_id: Some(group_id.clone()),
        coffee_id: Some(coffee_id.clone()),
        aliquot_id: Some(aliquot_id.clone()),
        species: Some(species.clone()),
        bean_state: Some(bean_state.clone()),
        roast_level: b.roast_level.clone().filter(|s| !s.is_empty()),
        mass_g: b.mass_g,
        operator_name: b.operator_name.clone().filter(|s| !s.is_empty()),
        protocol_ver: b.protocol_ver.clone().filter(|s| !s.is_empty()),
        run_order: b.run_order,
        heating_s: b.heating_s.or(Some(300)),
        duration_s: duration_s as i32,
        sample_period_ms: period as i32,
        warmup_min: b.warmup_min,
        room_t: b.room_t,
        room_rh: b.room_rh,
        divider_ratio: b.divider_ratio,
        notes,
        // Data mode uji tidak boleh tercampur dengan dataset riset.
        qc_flag: if b.dry_run { "REJECT".into() } else { "OK".into() },
        status: "queued".into(),
        dry_run: b.dry_run,
        ..Default::default()
    };
    st.store.insert_session(&session).await?;
    st.invalidate_list().await;

    let cmd = Command {
        id: st.next_command_id(),
        kind: "start".into(),
        measurement: Some(CommandMeasurement {
            measurement_id: measurement_id.clone(),
            daq_id,
            batch_id,
            group_id,
            coffee_id,
            aliquot_id,
            species,
            bean_state,
        }),
        duration_s: Some(duration_s),
        sample_period_ms: Some(period),
        allow_missing_sensors: Some(b.dry_run),
        issued_at: Utc::now(),
    };
    {
        let mut devices = st.devices.write().await;
        if let Some(dev) = devices.get_mut(&b.device_id) {
            dev.log("command", Some(&measurement_id), Some("Perintah mulai rekam dikirim"));
            dev.pending_command = Some(cmd.clone());
        }
    }
    st.active.write().await.insert(
        measurement_id.clone(),
        ActiveSession { device_id: b.device_id.clone(), queued_at: Instant::now(), started: false },
    );
    Ok(Json(json!({ "ok": true, "measurement_id": measurement_id, "command": cmd })))
}

// ───────────────────────────── Riwayat pengukuran ─────────────────────────────

#[derive(Deserialize, Default)]
pub struct ListQuery {
    pub search: Option<String>,
    pub coffee_id: Option<String>,
    pub species: Option<String>,
    pub bean_state: Option<String>,
    pub device_id: Option<String>,
    pub batch_id: Option<String>,
    pub status: Option<String>,
    pub qc_flag: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub sort: Option<String>,
    pub order: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

fn row_time(r: &MeasurementRow) -> Option<DateTime<Utc>> {
    r.session.started_at.or(r.session.created_at)
}

pub fn filter_rows(rows: &[MeasurementRow], q: &ListQuery) -> Vec<MeasurementRow> {
    let eq = |a: &Option<String>, b: &Option<String>| match b.as_deref().filter(|s| !s.is_empty()) {
        None => true,
        Some(want) => a.as_deref().map(|x| x.eq_ignore_ascii_case(want)).unwrap_or(false),
    };
    let from = q.from.as_deref().and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    let to = q.to.as_deref().and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    let search = q.search.as_deref().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
    let mut out: Vec<MeasurementRow> = rows
        .iter()
        .filter(|r| {
            let s = &r.session;
            if let Some(needle) = &search {
                let hay = format!(
                    "{} {} {} {}",
                    s.measurement_id,
                    s.coffee_id.as_deref().unwrap_or(""),
                    s.notes.as_deref().unwrap_or(""),
                    r.result.as_ref().and_then(|x| x.predicted_class.as_deref()).unwrap_or("")
                )
                .to_lowercase();
                if !hay.contains(needle) {
                    return false;
                }
            }
            let dev = Some(s.device_id.clone());
            let status = Some(s.status.clone());
            let qc = Some(s.qc_flag.clone());
            if !(eq(&s.coffee_id, &q.coffee_id)
                && eq(&s.species, &q.species)
                && eq(&s.bean_state, &q.bean_state)
                && eq(&dev, &q.device_id)
                && eq(&s.batch_id, &q.batch_id)
                && eq(&status, &q.status)
                && eq(&qc, &q.qc_flag))
            {
                return false;
            }
            // Tanggal difilter dalam WIB (UTC+7) — sesuai waktu lab.
            let day = row_time(r).map(|t| (t + chrono::Duration::hours(7)).date_naive());
            if let (Some(f), Some(d)) = (from, day) {
                if d < f {
                    return false;
                }
            }
            if let (Some(t), Some(d)) = (to, day) {
                if d > t {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    let desc = q.order.as_deref() != Some("asc");
    let key = q.sort.as_deref().unwrap_or("time");
    out.sort_by(|a, b| {
        let ord = match key {
            "measurement_id" => a.session.measurement_id.cmp(&b.session.measurement_id),
            "coffee_id" => a.session.coffee_id.cmp(&b.session.coffee_id),
            "device_id" => a.session.device_id.cmp(&b.session.device_id),
            "confidence" => {
                let ca = a.result.as_ref().and_then(|r| r.confidence).unwrap_or(-1.0);
                let cb = b.result.as_ref().and_then(|r| r.confidence).unwrap_or(-1.0);
                ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
            }
            "status" => a.session.status.cmp(&b.session.status),
            _ => row_time(a).cmp(&row_time(b)),
        };
        if desc { ord.reverse() } else { ord }
    });
    out
}

pub async fn list_measurements(State(st): State<Shared>, Query(q): Query<ListQuery>) -> ApiResult<Json<Value>> {
    let rows = st.measurements().await?;
    let filtered = filter_rows(&rows, &q);
    let size = q.page_size.unwrap_or(20).clamp(1, 500);
    let page = q.page.unwrap_or(1).max(1);
    let total = filtered.len();
    let items: Vec<_> = filtered.into_iter().skip((page - 1) * size).take(size).collect();
    Ok(Json(json!({ "items": items, "total": total, "page": page, "page_size": size })))
}

pub async fn get_measurement(State(st): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let rows = st.measurements().await?;
    let row = rows.iter().find(|r| r.session.measurement_id == id).ok_or_else(|| ApiError::not_found("Pengukuran tidak ditemukan"))?;
    Ok(Json(json!(row)))
}

pub async fn measurement_samples(State(st): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let rows = st.store.samples(&id).await?;
    Ok(Json(json!(rows)))
}

#[derive(Deserialize)]
pub struct QcBody {
    qc_flag: String,
    notes: Option<String>,
}

/// Data mentah TIDAK pernah dihapus (handbook 17.5) — pengukuran bermasalah ditandai QC.
pub async fn set_qc(State(st): State<Shared>, h: HeaderMap, Path(id): Path<String>, Json(b): Json<QcBody>) -> ApiResult<Json<Value>> {
    require_operator(&h, &st)?;
    let qc = b.qc_flag.to_uppercase();
    if !["OK", "SUSPECT", "REJECT"].contains(&qc.as_str()) {
        return Err(ApiError::bad_request("QC_Flag harus OK, SUSPECT, atau REJECT"));
    }
    if !st.store.set_qc(&id, &qc, b.notes.as_deref()).await? {
        return Err(ApiError::not_found("Pengukuran tidak ditemukan"));
    }
    st.invalidate_list().await;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Statistik & fitur ─────────────────────────────

pub async fn stats(State(st): State<Shared>) -> ApiResult<Json<Value>> {
    let rows = st.measurements().await?;
    let mut by_status: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_qc: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_class: BTreeMap<String, usize> = BTreeMap::new();
    let mut coffee: BTreeMap<String, Value> = BTreeMap::new();
    let mut batch_species: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut daq_species: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let (mut conf_sum, mut conf_n, mut inf_sum, mut inf_n) = (0.0, 0usize, 0.0, 0usize);

    for r in rows.iter() {
        let s = &r.session;
        *by_status.entry(s.status.clone()).or_default() += 1;
        *by_qc.entry(s.qc_flag.clone()).or_default() += 1;
        if s.status != "completed" || s.dry_run {
            continue;
        }
        let sp = s.species.clone().unwrap_or_else(|| "?".into());
        *batch_species.entry(s.batch_id.clone().unwrap_or_else(|| "?".into())).or_default().entry(sp.clone()).or_default() += 1;
        *daq_species.entry(s.daq_id.clone().unwrap_or_else(|| s.device_id.clone())).or_default().entry(sp.clone()).or_default() += 1;
        if let Some(cid) = &s.coffee_id {
            let e = coffee.entry(cid.clone()).or_insert_with(|| {
                json!({ "coffee_id": cid, "species": s.species, "bean_state": s.bean_state, "group_id": s.group_id,
                        "completed": 0, "aliquots": [], "devices": [], "batches": [] })
            });
            e["completed"] = json!(e["completed"].as_u64().unwrap_or(0) + 1);
            for (k, v) in [("aliquots", &s.aliquot_id), ("devices", &s.daq_id), ("batches", &s.batch_id)] {
                if let Some(v) = v {
                    let arr = e[k].as_array_mut().unwrap();
                    if !arr.iter().any(|x| x == v) {
                        arr.push(json!(v));
                    }
                }
            }
        }
        if let Some(res) = &r.result {
            if let Some(c) = res.predicted_class.clone() {
                *by_class.entry(c).or_default() += 1;
            }
            if let Some(c) = res.confidence {
                conf_sum += c;
                conf_n += 1;
            }
            if let Some(i) = res.inference_time_ms.filter(|x| *x > 0.0) {
                inf_sum += i;
                inf_n += 1;
            }
        }
    }
    let latest = rows
        .iter()
        .filter(|r| r.session.status == "completed" && r.result.is_some())
        .max_by_key(|r| r.session.finished_at.or(r.session.created_at));
    Ok(Json(json!({
        "total": rows.len(),
        "by_status": by_status,
        "by_qc": by_qc,
        "by_predicted_class": by_class,
        "coffees": coffee.into_values().collect::<Vec<_>>(),
        "contingency_batch_species": batch_species,
        "contingency_daq_species": daq_species,
        "mean_confidence": if conf_n > 0 { Some(conf_sum / conf_n as f64) } else { None },
        "mean_inference_ms": if inf_n > 0 { Some(inf_sum / inf_n as f64) } else { None },
        "latest": latest,
    })))
}

/// Vektor fitur ringkas (late mean + peak) untuk PCA eksploratif di dashboard.
pub async fn features(State(st): State<Shared>) -> ApiResult<Json<Value>> {
    let rows = st.measurements().await?;
    let out: Vec<Value> = rows
        .iter()
        .filter(|r| r.session.status == "completed" && !r.session.dry_run)
        .filter_map(|r| {
            let res = r.result.as_ref()?;
            let late: Vec<f64> = res.sensor_late_mean.iter().map(|v| v.unwrap_or(f64::NAN)).collect();
            let peak: Vec<f64> = res.sensor_peak_abs.iter().map(|v| v.unwrap_or(f64::NAN)).collect();
            if late.len() != SENSOR_COUNT || peak.len() != SENSOR_COUNT || late.iter().chain(&peak).any(|x| !x.is_finite()) {
                return None;
            }
            let s = &r.session;
            Some(json!({
                "measurement_id": s.measurement_id, "coffee_id": s.coffee_id, "daq_id": s.daq_id.clone().unwrap_or(s.device_id.clone()),
                "batch_id": s.batch_id, "species": s.species, "bean_state": s.bean_state, "qc_flag": s.qc_flag,
                "predicted_class": res.predicted_class, "late_mean": late, "peak_abs": peak,
            }))
        })
        .collect();
    Ok(Json(json!(out)))
}

/// Tandai sesi yang macet: perintah tidak pernah diambil, atau perangkat hilang saat merekam.
pub async fn sweeper(st: Shared) {
    let mut tick = tokio::time::interval(Duration::from_secs(15));
    loop {
        tick.tick().await;
        let active: Vec<(String, ActiveSession)> = st.active.read().await.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        if active.is_empty() {
            continue;
        }
        // (online, detik sejak terlihat, measurement aktif di perangkat, idle tanpa antrian unggah)
        let devices: HashMap<String, (bool, f64, Option<String>, bool)> = st
            .devices
            .read()
            .await
            .iter()
            .map(|(k, d)| {
                let idle = d.hb.state == "idle" && d.hb.pending_uploads.unwrap_or(0) == 0;
                (k.clone(), (d.online(), d.last_seen_at.elapsed().as_secs_f64(), d.hb.measurement_id.clone(), idle))
            })
            .collect();
        for (mid, a) in active {
            let dev = devices.get(&a.device_id);
            let offline_for = dev.map(|d| if d.0 { 0.0 } else { d.1 }).unwrap_or(f64::MAX);
            let verdict = if !a.started && a.queued_at.elapsed() > Duration::from_secs(90) {
                Some((StatusChange::Failed, "Perangkat tidak mengambil perintah dalam 90 detik"))
            } else if offline_for > 120.0 && a.queued_at.elapsed() > Duration::from_secs(120) {
                Some((StatusChange::Interrupted, "Perangkat offline lebih dari 2 menit saat merekam"))
            } else if a.started
                && a.queued_at.elapsed() > Duration::from_secs(180)
                && dev.map(|d| d.0 && d.3 && d.2.as_deref() != Some(mid.as_str())).unwrap_or(false)
            {
                Some((StatusChange::Interrupted, "Perangkat sudah idle tanpa mengirim hasil"))
            } else {
                None
            };
            if let Some((change, msg)) = verdict {
                tracing::warn!("Sesi {mid}: {msg}");
                if st.store.set_status(&mid, change, Some(msg)).await.is_ok() {
                    st.active.write().await.remove(&mid);
                    if let Some(d) = st.devices.write().await.get_mut(&a.device_id) {
                        d.log("interrupted", Some(&mid), Some(msg));
                        if d.pending_command.as_ref().and_then(|c| c.measurement.as_ref()).map(|m| m.measurement_id == mid).unwrap_or(false) {
                            d.pending_command = None;
                        }
                    }
                    st.invalidate_list().await;
                }
            }
        }
    }
}

/// Muat ulang sesi yang belum selesai setelah server restart (mis. redeploy Railway).
pub async fn restore_active(st: &Shared) {
    if let Ok(rows) = st.store.list().await {
        let mut active = st.active.write().await;
        for r in rows.iter().filter(|r| r.session.status == "queued" || r.session.status == "recording") {
            active.insert(
                r.session.measurement_id.clone(),
                ActiveSession { device_id: r.session.device_id.clone(), queued_at: Instant::now(), started: r.session.status == "recording" },
            );
        }
        if !active.is_empty() {
            tracing::info!("{} sesi aktif dipulihkan", active.len());
        }
    }
}
