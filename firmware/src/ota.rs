//! OTA firmware lewat ThingsBoard (HTTP Device API).
//!
//! 1. GET  {TB}/api/v1/{TOKEN}/attributes?sharedKeys=fw_title,fw_version,...
//! 2. Bila fw_title cocok & fw_version berbeda → GET {TB}/api/v1/{TOKEN}/firmware?title=&version=
//! 3. Tulis ke partisi OTA sambil menghitung SHA-256, bandingkan dengan fw_checksum
//! 4. Lapor telemetry fw_state: DOWNLOADING → DOWNLOADED → VERIFIED → UPDATING → (restart) → UPDATED
//!
//! CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y: image baru harus "dikonfirmasi" (confirm_boot)
//! setelah berhasil terhubung ke server. Bila crash/restart sebelum itu, bootloader
//! otomatis kembali ke firmware lama.

use crate::config::*;
use crate::net::Http;
use crate::shared::{DevState, SharedRef};
use anyhow::{anyhow, bail, Context, Result};
use esp_idf_svc::{
    http::{
        client::{Configuration as HttpConfig, EspHttpConnection},
        Method,
    },
    nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault},
    ota::{EspOta, SlotState},
};
use log::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

const NVS_NS: &str = "enose";
const KEY_PENDING: &str = "ota_target";

fn tb(path: &str) -> String {
    format!("{}/api/v1/{}{}", TB_URL.trim_end_matches('/'), TB_TOKEN, path)
}

fn telemetry(http: &mut Http, v: Value) {
    if let Err(e) = http.post_json(&tb("/telemetry"), &v.to_string(), &[]) {
        warn!("Telemetry ThingsBoard gagal: {e:#}");
    }
}

fn set_progress(shared: &SharedRef, state: &str, progress: f32, msg: &str) {
    let mut s = shared.lock().unwrap();
    s.ota_state = state.into();
    s.ota_progress = progress;
    s.ota_message = Some(msg.into());
}

/// Dipanggil sekali setelah heartbeat pertama sukses.
pub fn confirm_boot(nvs: &EspDefaultNvsPartition) {
    if let Ok(mut ota) = EspOta::new() {
        if let Ok(slot) = ota.get_running_slot() {
            if slot.state != SlotState::Valid && slot.state != SlotState::Factory {
                match ota.mark_running_slot_valid() {
                    Ok(_) => info!("OTA: firmware v{FW_VERSION} dikonfirmasi valid"),
                    Err(e) => warn!("OTA: gagal menandai slot valid: {e}"),
                }
            }
        }
    }
    if TB_TOKEN.is_empty() {
        return;
    }
    // Laporkan versi berjalan + hasil OTA sebelumnya ke ThingsBoard
    let mut http = Http::new();
    let pending = EspNvs::<NvsDefault>::new(nvs.clone(), NVS_NS, true).ok().and_then(|n| {
        let mut buf = [0u8; 32];
        let v = n.get_str(KEY_PENDING, &mut buf).ok().flatten().map(str::to_string);
        let _ = n.remove(KEY_PENDING);
        v
    });
    let mut t = json!({ "current_fw_title": FW_TITLE, "current_fw_version": FW_VERSION });
    match pending.as_deref() {
        Some(v) if v == FW_VERSION => t["fw_state"] = json!("UPDATED"),
        Some(v) => {
            t["fw_state"] = json!("FAILED");
            t["fw_error"] = json!(format!("Rollback: target {v}, berjalan {FW_VERSION}"));
        }
        None => {}
    }
    telemetry(&mut http, t);
}

/// Periksa atribut firmware di ThingsBoard; unduh & pasang bila ada versi baru.
pub fn check_and_update(shared: &SharedRef, nvs: &EspDefaultNvsPartition) -> Result<()> {
    let mut http = Http::new();
    let (status, body) = http.request(
        Method::Get,
        &tb("/attributes?sharedKeys=fw_title,fw_version,fw_size,fw_checksum,fw_checksum_algorithm"),
        None,
        &[],
    )?;
    if status != 200 {
        bail!("atribut ThingsBoard → HTTP {status} (cek TB_TOKEN)");
    }
    let v: Value = serde_json::from_slice(&body)?;
    let sh = &v["shared"];
    let (Some(title), Some(version)) = (sh["fw_title"].as_str(), sh["fw_version"].as_str()) else {
        return Ok(()); // belum ada firmware di-assign
    };
    if title != FW_TITLE {
        warn!("OTA diabaikan: fw_title '{title}' ≠ '{FW_TITLE}'");
        return Ok(());
    }
    if version == FW_VERSION {
        return Ok(());
    }
    let size = sh["fw_size"].as_u64().unwrap_or(0);
    let checksum = sh["fw_checksum"].as_str().unwrap_or("").to_lowercase();
    let algo = sh["fw_checksum_algorithm"].as_str().unwrap_or("SHA256");
    info!("OTA: versi baru {version} ({size} byte)");

    shared.lock().unwrap().state = DevState::Ota;
    let result = download_and_flash(shared, &mut http, title, version, size, &checksum, algo, nvs);
    if let Err(e) = &result {
        error!("OTA gagal: {e:#}");
        set_progress(shared, "FAILED", 0.0, &format!("{e:#}"));
        telemetry(&mut http, json!({ "fw_state": "FAILED", "fw_error": format!("{e:#}") }));
        shared.lock().unwrap().state = DevState::Idle;
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn download_and_flash(
    shared: &SharedRef,
    http: &mut Http,
    title: &str,
    version: &str,
    size: u64,
    checksum: &str,
    algo: &str,
    nvs: &EspDefaultNvsPartition,
) -> Result<()> {
    telemetry(http, json!({ "current_fw_title": FW_TITLE, "current_fw_version": FW_VERSION,
                            "target_fw_title": title, "target_fw_version": version, "fw_state": "DOWNLOADING" }));
    set_progress(shared, "DOWNLOADING", 0.0, "Mengunduh firmware");

    let mut ota = EspOta::new().context("EspOta")?;
    let mut update = ota.initiate_update().context("initiate_update")?;
    let mut conn = EspHttpConnection::new(&HttpConfig {
        timeout: Some(Duration::from_secs(30)),
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        buffer_size: Some(4096),
        ..Default::default()
    })?;
    let url = tb(&format!("/firmware?title={}&version={}", urlencode(title), urlencode(version)));
    let mut hasher = Sha256::new();
    let mut got: u64 = 0;
    let dl: Result<()> = (|| {
        conn.initiate_request(Method::Get, &url, &[])?;
        conn.initiate_response()?;
        if conn.status() != 200 {
            bail!("unduh firmware → HTTP {}", conn.status());
        }
        let mut buf = vec![0u8; 4096];
        let mut last_report = 0u64;
        loop {
            let n = conn.read(&mut buf)?;
            if n == 0 {
                break;
            }
            if got == 0 && buf[0] != 0xE9 {
                bail!("bukan image ESP32 (magic byte salah)");
            }
            update.write(&buf[..n])?;
            hasher.update(&buf[..n]);
            got += n as u64;
            if size > 0 && got - last_report > 64 * 1024 {
                last_report = got;
                set_progress(shared, "DOWNLOADING", got as f32 / size as f32 * 0.8, &format!("{} / {} KB", got / 1024, size / 1024));
            }
        }
        if size > 0 && got != size {
            bail!("ukuran tidak cocok: {got} dari {size} byte");
        }
        Ok(())
    })();
    if let Err(e) = dl {
        let _ = update.abort();
        return Err(e);
    }
    telemetry(http, json!({ "fw_state": "DOWNLOADED" }));
    set_progress(shared, "DOWNLOADED", 0.85, "Memverifikasi checksum");

    let digest = hex(&hasher.finalize());
    if algo.eq_ignore_ascii_case("SHA256") && !checksum.is_empty() && digest != checksum {
        let _ = update.abort();
        return Err(anyhow!("checksum SHA-256 tidak cocok"));
    }
    telemetry(http, json!({ "fw_state": "VERIFIED" }));
    telemetry(http, json!({ "fw_state": "UPDATING" }));
    set_progress(shared, "UPDATING", 0.95, "Menulis partisi & restart");

    if let Ok(n) = EspNvs::<NvsDefault>::new(nvs.clone(), NVS_NS, true) {
        let _ = n.set_str(KEY_PENDING, version);
    }
    update.complete().context("finalisasi OTA")?;
    info!("OTA selesai — restart ke v{version}");
    std::thread::sleep(Duration::from_millis(500));
    esp_idf_svc::hal::reset::restart();
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}
