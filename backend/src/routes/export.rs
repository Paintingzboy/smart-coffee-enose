//! Ekspor dataset dalam format handbook (bagian 17): CSV mentah per pengukuran,
//! measurement_metadata.csv, dan paket ZIP lengkap dengan checksums SHA-256.

use super::dashboard::{filter_rows, ListQuery};
use crate::error::{ApiError, ApiResult};
use crate::models::*;
use crate::state::Shared;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderValue},
    response::Response,
};
use chrono::{DateTime, FixedOffset, Utc};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;

const PGA_FSR_V: f64 = 4.096;

fn wib(t: Option<DateTime<Utc>>) -> String {
    let tz = FixedOffset::east_opt(7 * 3600).unwrap();
    t.map(|t| t.with_timezone(&tz).format("%Y-%m-%dT%H:%M:%S%:z").to_string()).unwrap_or_default()
}

fn esc(v: &str) -> String {
    if v.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v.to_string()
    }
}
fn os(v: &Option<String>) -> String {
    v.as_deref().map(esc).unwrap_or_default()
}
fn of(v: Option<f64>, d: usize) -> String {
    v.filter(|x| x.is_finite()).map(|x| format!("{x:.d$}")).unwrap_or_default()
}
fn oi(v: Option<i32>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

fn file_response(bytes: Vec<u8>, content_type: &str, filename: &str) -> Response {
    let mut r = Response::new(Body::from(bytes));
    r.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_str(content_type).unwrap());
    r.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")).unwrap(),
    );
    r
}

/// CSV mentah: kolom sama persis dengan handbook bagian 17.4.
pub fn raw_csv(samples: &[SampleRow]) -> String {
    let mut out = String::from("t_s,timestamp,");
    out.push_str(&SENSOR_KEYS.iter().map(|k| format!("{k}_raw")).collect::<Vec<_>>().join(","));
    out.push(',');
    out.push_str(&SENSOR_KEYS.iter().map(|k| format!("{k}_v")).collect::<Vec<_>>().join(","));
    out.push_str(",temp_c,rh_pct,dht_age_s\n");
    for s in samples {
        let raw: Vec<String> = (0..SENSOR_COUNT).map(|k| s.raw.get(k).copied().flatten().map(|x| x.to_string()).unwrap_or_default()).collect();
        let v: Vec<String> = (0..SENSOR_COUNT).map(|k| of(s.v.get(k).copied().flatten(), 6)).collect();
        out.push_str(&format!(
            "{:.3},{},{},{},{},{},{}\n",
            s.t_s,
            os(&s.ts),
            raw.join(","),
            v.join(","),
            of(s.temp_c, 1),
            of(s.rh_pct, 1),
            oi(s.dht_age_s)
        ));
    }
    out
}

pub async fn measurement_csv(State(st): State<Shared>, Path(id): Path<String>) -> ApiResult<Response> {
    let id = id.trim_end_matches(".csv").to_string();
    let samples = st.store.samples(&id).await?;
    if samples.is_empty() {
        return Err(ApiError::not_found("Belum ada data mentah untuk pengukuran ini"));
    }
    Ok(file_response(raw_csv(&samples).into_bytes(), "text/csv; charset=utf-8", &format!("{id}.csv")))
}

/// measurement_metadata.csv sesuai tabel handbook 17.3 (+ kolom hasil model).
fn metadata_csv(rows: &[MeasurementRow]) -> String {
    // Interval_min: jeda sejak pengukuran sebelumnya pada perangkat yang sama.
    let mut prev: HashMap<String, DateTime<Utc>> = HashMap::new();
    let mut sorted: Vec<&MeasurementRow> = rows.iter().collect();
    sorted.sort_by_key(|r| r.session.started_at.or(r.session.created_at));
    let mut interval: HashMap<String, f64> = HashMap::new();
    for r in &sorted {
        if let Some(t) = r.session.started_at {
            if let Some(p) = prev.get(&r.session.device_id) {
                interval.insert(r.session.measurement_id.clone(), (t - *p).num_seconds() as f64 / 60.0);
            }
            prev.insert(r.session.device_id.clone(), t);
        }
    }
    let mut out = String::from(
        "Measurement_ID,DAQ_ID,Batch_ID,Group_ID,Coffee_ID,Species,Bean_State,Aliquot_ID,Mass_g,Timestamp,\
Temperature,Humidity,Room_T,Room_RH,Warmup_min,Interval_min,Run_Order,Heating_s,Duration_s,Fs_Hz,PGA_FSR_V,\
Divider_Ratio,Protocol_Ver,Operator,QC_Flag,Notes,Status,Device_ID,Roast_Level,Samples,Predicted_Class,Confidence,\
Model_Version,FW_Version,Inference_ms\n",
    );
    for r in sorted {
        let s = &r.session;
        let res = r.result.as_ref();
        let fields = [
            esc(&s.measurement_id),
            os(&s.daq_id),
            os(&s.batch_id),
            os(&s.group_id),
            os(&s.coffee_id),
            os(&s.species),
            os(&s.bean_state),
            os(&s.aliquot_id),
            of(s.mass_g, 2),
            wib(s.started_at.or(s.created_at)),
            of(res.and_then(|x| x.temperature), 2),
            of(res.and_then(|x| x.humidity), 2),
            of(s.room_t, 1),
            of(s.room_rh, 1),
            oi(s.warmup_min),
            of(interval.get(&s.measurement_id).copied(), 1),
            oi(s.run_order),
            oi(s.heating_s),
            s.duration_s.to_string(),
            format!("{:.3}", 1000.0 / s.sample_period_ms.max(1) as f64),
            format!("{PGA_FSR_V}"),
            of(s.divider_ratio, 4),
            os(&s.protocol_ver),
            os(&s.operator_name),
            esc(&s.qc_flag),
            os(&s.notes),
            esc(&s.status),
            esc(&s.device_id),
            os(&s.roast_level),
            s.samples_received.to_string(),
            os(&res.and_then(|x| x.predicted_class.clone())),
            of(res.and_then(|x| x.confidence), 4),
            os(&res.and_then(|x| x.model_version.clone())),
            os(&res.and_then(|x| x.fw_version.clone())),
            of(res.and_then(|x| x.inference_time_ms), 2),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out
}

pub async fn export_metadata(State(st): State<Shared>, Query(q): Query<ListQuery>) -> ApiResult<Response> {
    let rows = st.measurements().await?;
    let rows = filter_rows(&rows, &q);
    Ok(file_response(metadata_csv(&rows).into_bytes(), "text/csv; charset=utf-8", "measurement_metadata.csv"))
}

pub async fn export_json(State(st): State<Shared>, Query(q): Query<ListQuery>) -> ApiResult<Response> {
    let rows = st.measurements().await?;
    let rows = filter_rows(&rows, &q);
    let body = serde_json::to_vec_pretty(&rows).map_err(anyhow::Error::from)?;
    Ok(file_response(body, "application/json", "smart_coffee_enose_measurements.json"))
}

const DATA_DICTIONARY: &str = r#"# Data dictionary — Smart Coffee E-Nose

## 01_raw_data/<Measurement_ID>.csv (1 baris per sampel)
| Kolom | Satuan | Keterangan |
|---|---|---|
| t_s | s | Waktu relatif sejak awal rekaman (timer monotonik ESP32) |
| timestamp | ISO 8601 UTC | Waktu absolut (NTP) |
| mq3_raw … tgs2620_raw | kode ADC | Keluaran ADS1115 16-bit, single-ended (0..32767) |
| mq3_v … tgs2620_v | V | raw × 4.096 / 32768 (PGA ±4.096 V) — tegangan di pin ADC, sebelum koreksi pembagi tegangan |
| temp_c | °C | DHT22, di-hold ke grid sampling (zero-order hold) |
| rh_pct | % | DHT22, di-hold ke grid sampling |
| dht_age_s | s | Umur pembacaan DHT22 terakhir yang di-hold |

Urutan kanal: S1 MQ-3, S2 MQ-6, S3 MQ-7, S4 MQ-135 (ADS1115 0x48 A0–A3); S5 TGS2600, S6 TGS2602, S7 TGS2611, S8 TGS2620 (ADS1115 0x49 A0–A3).

## 02_metadata/measurement_metadata.csv
Kolom mengikuti handbook bagian 17.3. Kolom tambahan: Status, Device_ID, Roast_Level, Samples, Predicted_Class, Confidence (keluaran model untuk satu inferensi — BUKAN akurasi), Model_Version, FW_Version, Inference_ms.

QC_Flag: OK / SUSPECT / REJECT. Pengukuran mode uji tanpa sensor otomatis REJECT.

## 02_metadata/checksums.txt
SHA-256 setiap file mentah (format `sha256sum`).
"#;

pub async fn export_zip(State(st): State<Shared>, Query(q): Query<ListQuery>) -> ApiResult<Response> {
    let rows = st.measurements().await?;
    let rows = filter_rows(&rows, &q);
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut checksums = String::new();
        let mut included = Vec::new();
        for r in &rows {
            if r.session.samples_received == 0 && r.result.is_none() {
                continue;
            }
            let samples = st.store.samples(&r.session.measurement_id).await?;
            if samples.is_empty() {
                continue;
            }
            let csv = raw_csv(&samples);
            let name = format!("{}.csv", r.session.measurement_id);
            checksums.push_str(&format!("{}  01_raw_data/{}\n", hex::encode(Sha256::digest(csv.as_bytes())), name));
            zip.start_file(format!("01_raw_data/{name}"), opts).map_err(anyhow::Error::from)?;
            zip.write_all(csv.as_bytes()).map_err(anyhow::Error::from)?;
            included.push(r.clone());
        }
        zip.start_file("02_metadata/measurement_metadata.csv", opts).map_err(anyhow::Error::from)?;
        zip.write_all(metadata_csv(&included).as_bytes()).map_err(anyhow::Error::from)?;
        zip.start_file("02_metadata/checksums.txt", opts).map_err(anyhow::Error::from)?;
        zip.write_all(checksums.as_bytes()).map_err(anyhow::Error::from)?;
        zip.start_file("02_metadata/data_dictionary.md", opts).map_err(anyhow::Error::from)?;
        zip.write_all(DATA_DICTIONARY.as_bytes()).map_err(anyhow::Error::from)?;
        zip.finish().map_err(anyhow::Error::from)?;
    }
    let stamp = Utc::now().format("%Y%m%d_%H%M");
    Ok(file_response(buf.into_inner(), "application/zip", &format!("PROJECT_COFFEE_ENOSE_dataset_{stamp}.zip")))
}
