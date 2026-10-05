//! Endpoint yang dipanggil firmware ESP32-S3.

use super::require_device;
use crate::error::{ApiError, ApiResult};
use crate::models::*;
use crate::state::{DeviceLive, Shared};
use crate::store::StatusChange;
use axum::{extract::State, http::HeaderMap, Json};
use chrono::Utc;
use serde_json::{json, Value};
use std::time::Instant;

/// Heartbeat ±3 detik. Respons membawa perintah tertunda (bila ada) sehingga
/// ESP32 di balik NAT/WiFi kampus tetap bisa dikendalikan tanpa IP publik.
pub async fn heartbeat(State(st): State<Shared>, h: HeaderMap, Json(hb): Json<Heartbeat>) -> ApiResult<Json<Value>> {
    require_device(&h, &st)?;
    if hb.device_id.is_empty() || hb.device_id.len() > 50 {
        return Err(ApiError::bad_request("device_id tidak valid"));
    }
    let mut interrupted: Option<String> = None;
    let command = {
        let mut devices = st.devices.write().await;
        let dev = devices.entry(hb.device_id.clone()).or_insert_with(|| {
            let mut d = DeviceLive::new(hb.clone());
            d.log("online", None, Some("Perangkat pertama kali terhubung"));
            d
        });
        let was_offline = !dev.online();
        // Deteksi reboot: uptime mengecil → pengukuran yang sedang berjalan terputus.
        let rebooted = matches!((dev.hb.uptime_s, hb.uptime_s), (Some(old), Some(new)) if new + 5 < old);
        if rebooted {
            dev.log("reboot", None, Some("Perangkat restart"));
            if dev.hb.state == "recording" {
                interrupted = dev.hb.measurement_id.clone();
            }
        } else if was_offline {
            dev.log("online", None, Some("Perangkat terhubung kembali"));
        }
        if let (Some(cmd), Some(ack)) = (dev.pending_command.clone(), hb.last_ack) {
            if cmd.id == ack {
                let mid = cmd.measurement.as_ref().map(|m| m.measurement_id.clone());
                dev.log("ack", mid.as_deref(), Some(&format!("Perintah '{}' diterima perangkat", cmd.kind)));
                dev.pending_command = None;
            }
        }
        if let Some(live) = &hb.live {
            if hb.state != "recording" {
                dev.push_live(live);
            }
        }
        if dev.hb.last_error != hb.last_error {
            if let Some(e) = &hb.last_error {
                dev.log("error", hb.measurement_id.as_deref(), Some(e));
            }
        }
        dev.hb = hb.clone();
        dev.last_seen = Utc::now();
        dev.last_seen_at = Instant::now();
        dev.pending_command.clone()
    };

    // Tandai sesi "recording" saat perangkat mulai merekam.
    if hb.state == "recording" {
        if let Some(mid) = &hb.measurement_id {
            let mut active = st.active.write().await;
            let entry = active.entry(mid.clone()).or_insert_with(|| crate::state::ActiveSession {
                device_id: hb.device_id.clone(),
                queued_at: Instant::now(),
                started: false,
            });
            if !entry.started {
                entry.started = true;
                drop(active);
                st.store.set_status(mid, StatusChange::Recording, None).await.ok();
                st.invalidate_list().await;
            }
        }
    }
    if let Some(mid) = interrupted {
        st.store.set_status(&mid, StatusChange::Interrupted, Some("Perangkat restart saat merekam")).await.ok();
        st.active.write().await.remove(&mid);
        st.invalidate_list().await;
    }

    Ok(Json(json!({ "server_time": Utc::now().to_rfc3339(), "command": command })))
}

/// Potongan data mentah (dikirim tiap ±5 detik saat merekam).
pub async fn samples(State(st): State<Shared>, h: HeaderMap, Json(batch): Json<SampleBatch>) -> ApiResult<Json<Value>> {
    require_device(&h, &st)?;
    if batch.rows.len() > 600 {
        return Err(ApiError::bad_request("maksimal 600 baris per batch"));
    }
    for r in &batch.rows {
        if r.raw.len() > SENSOR_COUNT || r.v.len() > SENSOR_COUNT {
            return Err(ApiError::bad_request("jumlah kanal melebihi 8"));
        }
    }
    {
        let mut devices = st.devices.write().await;
        if let Some(dev) = devices.get_mut(&batch.device_id) {
            dev.push_samples(&batch.measurement_id, &batch.rows);
        }
    }
    st.store.insert_samples(&batch.measurement_id, &batch.rows).await?;
    Ok(Json(json!({ "ok": true, "stored": batch.rows.len() })))
}

/// Event siklus hidup pengukuran (started/completed/failed/cancelled) dan event lain.
pub async fn event(State(st): State<Shared>, h: HeaderMap, Json(ev): Json<DeviceEvent>) -> ApiResult<Json<Value>> {
    require_device(&h, &st)?;
    {
        let mut devices = st.devices.write().await;
        if let Some(dev) = devices.get_mut(&ev.device_id) {
            dev.log(&ev.event, ev.measurement_id.as_deref(), ev.message.as_deref());
            if ev.event == "zero_baseline" {
                if let Some(v) = &ev.values {
                    dev.zero_baseline = Some((Utc::now(), v.clone()));
                }
            }
        }
    }
    if let Some(mid) = &ev.measurement_id {
        let change = match ev.event.as_str() {
            "started" => Some(StatusChange::Recording),
            "completed" => Some(StatusChange::Completed),
            "failed" => Some(StatusChange::Failed),
            "cancelled" => Some(StatusChange::Cancelled),
            _ => None,
        };
        if let Some(change) = change {
            st.store.set_status(mid, change, ev.message.as_deref()).await?;
            if change != StatusChange::Recording {
                st.active.write().await.remove(mid);
            } else if let Some(a) = st.active.write().await.get_mut(mid) {
                a.started = true;
            }
            st.invalidate_list().await;
        }
    }
    Ok(Json(json!({ "ok": true })))
}

/// Hasil akhir (fitur ringkas + inferensi TinyML). Format kompatibel dengan firmware lama.
pub async fn reading(State(st): State<Shared>, h: HeaderMap, Json(r): Json<Reading>) -> ApiResult<(axum::http::StatusCode, Json<Value>)> {
    require_device(&h, &st)?;
    if r.measurement_id.is_empty() {
        return Err(ApiError::bad_request("measurement_id kosong"));
    }
    let inserted = st.store.insert_reading(&r).await?;
    st.store.set_status(&r.measurement_id, StatusChange::Completed, None).await?;
    st.active.write().await.remove(&r.measurement_id);
    {
        let mut devices = st.devices.write().await;
        if let Some(dev) = devices.get_mut(&r.device_id) {
            let msg = format!(
                "{} (confidence {:.2})",
                r.predicted_class.as_deref().unwrap_or("-"),
                r.confidence.unwrap_or(0.0)
            );
            dev.log("result", Some(&r.measurement_id), Some(&msg));
        }
    }
    st.invalidate_list().await;
    let code = if inserted { axum::http::StatusCode::CREATED } else { axum::http::StatusCode::OK };
    Ok((code, Json(json!({ "ok": true, "duplicate": !inserted }))))
}
