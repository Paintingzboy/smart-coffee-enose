//! OTA firmware lewat ThingsBoard: unggah .bin → assign ke device → ESP32 mengunduh.

use super::require_operator;
use crate::error::{ApiError, ApiResult};
use crate::models::Command;
use crate::state::Shared;
use crate::thingsboard::ThingsBoard;
use axum::{
    extract::{Multipart, Path, State},
    http::HeaderMap,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

fn tb(st: &Shared) -> Result<&ThingsBoard, ApiError> {
    st.tb.as_ref().ok_or_else(|| ApiError::unavailable("ThingsBoard belum dikonfigurasi (set TB_URL dan kredensial)"))
}

pub async fn packages(State(st): State<Shared>) -> ApiResult<Json<Value>> {
    let list = tb(&st)?.list_packages().await?;
    Ok(Json(json!(list)))
}

/// Status firmware satu perangkat: dari ThingsBoard (fw_state) + progres unduhan dari heartbeat.
pub async fn device_status(State(st): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let live = st.devices.read().await.get(&id).map(|d| (d.hb.fw_version.clone(), d.hb.ota.clone(), d.online()));
    let tb_part = match &st.tb {
        None => json!({ "configured": false }),
        Some(tb) => match tb.find_device(&id).await {
            Ok(dev) => {
                let telemetry = tb.fw_telemetry(&dev.id).await.unwrap_or(Value::Null);
                json!({ "configured": true, "found": true, "device": dev, "telemetry": telemetry })
            }
            Err(e) => json!({ "configured": true, "found": false, "error": format!("{e:#}") }),
        },
    };
    Ok(Json(json!({
        "device_id": id,
        "fw_title": st.cfg.fw_title,
        "running_version": live.as_ref().and_then(|l| l.0.clone()),
        "online": live.as_ref().map(|l| l.2).unwrap_or(false),
        "progress": live.and_then(|l| l.1),
        "thingsboard": tb_part,
    })))
}

/// Multipart: file (.bin), version, device_id, [title], [assign=true]
pub async fn upload(State(st): State<Shared>, h: HeaderMap, mut mp: Multipart) -> ApiResult<Json<Value>> {
    require_operator(&h, &st)?;
    let tb = tb(&st)?;
    let (mut file, mut file_name, mut version, mut device_id, mut title, mut assign) =
        (None::<Vec<u8>>, String::from("firmware.bin"), None, None, None, true);
    while let Some(field) = mp.next_field().await.map_err(|e| ApiError::bad_request(e.to_string()))? {
        match field.name().unwrap_or_default() {
            "file" => {
                if let Some(n) = field.file_name() {
                    file_name = n.to_string();
                }
                file = Some(field.bytes().await.map_err(|e| ApiError::bad_request(e.to_string()))?.to_vec());
            }
            "version" => version = Some(field.text().await.unwrap_or_default().trim().to_string()),
            "device_id" => device_id = Some(field.text().await.unwrap_or_default().trim().to_string()),
            "title" => title = Some(field.text().await.unwrap_or_default().trim().to_string()),
            "assign" => assign = field.text().await.unwrap_or_default().trim() != "false",
            _ => {}
        }
    }
    let file = file.ok_or_else(|| ApiError::bad_request("File firmware (.bin) belum dipilih"))?;
    let version = version.filter(|v| !v.is_empty()).ok_or_else(|| ApiError::bad_request("Versi firmware wajib diisi"))?;
    let device_id = device_id.filter(|v| !v.is_empty()).ok_or_else(|| ApiError::bad_request("Pilih perangkat tujuan"))?;
    let title = title.filter(|v| !v.is_empty()).unwrap_or_else(|| st.cfg.fw_title.clone());
    if !file_name.to_lowercase().ends_with(".bin") {
        return Err(ApiError::bad_request("File harus berekstensi .bin (hasil `espflash save-image`)"));
    }
    // Byte pertama image aplikasi ESP-IDF selalu 0xE9.
    if file.first() != Some(&0xE9) || file.len() < 1024 {
        return Err(ApiError::bad_request("Bukan image aplikasi ESP32 yang valid (magic byte 0xE9 tidak ditemukan)"));
    }
    if file.len() > 0x1F0000 {
        return Err(ApiError::bad_request("Ukuran firmware melebihi partisi OTA (1,94 MB)"));
    }
    let running = st.devices.read().await.get(&device_id).and_then(|d| d.hb.fw_version.clone());
    if running.as_deref() == Some(version.as_str()) {
        return Err(ApiError::bad_request(format!("Perangkat sudah menjalankan versi {version}. Naikkan versi di Cargo.toml.")));
    }

    let dev = tb.find_device(&device_id).await?;
    let profile = dev.device_profile_id.clone().ok_or_else(|| ApiError::bad_request("Device ThingsBoard tanpa profil"))?;
    let pkg = tb.upload_package(&title, &version, &profile, &file_name, file).await?;
    if assign {
        tb.assign(&dev, &pkg.id).await?;
        push_check(&st, &device_id).await;
    }
    Ok(Json(json!({ "ok": true, "package": pkg, "assigned": assign })))
}

#[derive(Deserialize)]
pub struct AssignBody {
    device_id: String,
    package_id: String,
}

pub async fn assign(State(st): State<Shared>, h: HeaderMap, Json(b): Json<AssignBody>) -> ApiResult<Json<Value>> {
    require_operator(&h, &st)?;
    let tb = tb(&st)?;
    let dev = tb.find_device(&b.device_id).await?;
    tb.assign(&dev, &b.package_id).await?;
    push_check(&st, &b.device_id).await;
    Ok(Json(json!({ "ok": true })))
}

/// Minta perangkat segera memeriksa atribut firmware (tanpa menunggu interval polling).
async fn push_check(st: &Shared, device_id: &str) {
    let id = st.next_command_id();
    let mut devices = st.devices.write().await;
    if let Some(d) = devices.get_mut(device_id) {
        if d.online() && d.hb.state != "recording" && d.pending_command.is_none() {
            d.pending_command = Some(Command {
                id,
                kind: "check_ota".into(),
                measurement: None,
                duration_s: None,
                sample_period_ms: None,
                allow_missing_sensors: None,
                issued_at: Utc::now(),
            });
        }
        d.log("ota", None, Some("Firmware baru di-assign lewat ThingsBoard"));
    }
}
