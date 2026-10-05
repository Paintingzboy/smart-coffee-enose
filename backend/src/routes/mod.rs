pub mod dashboard;
pub mod device;
pub mod export;
pub mod ota;

use crate::error::ApiError;
use crate::state::AppState;
use axum::http::HeaderMap;

fn header<'a>(h: &'a HeaderMap, name: &str) -> Option<&'a str> {
    h.get(name).and_then(|v| v.to_str().ok())
}

/// Endpoint yang dipanggil ESP32 wajib membawa header `X-Device-Key`.
pub fn require_device(h: &HeaderMap, st: &AppState) -> Result<(), ApiError> {
    match header(h, "x-device-key") {
        Some(k) if k == st.cfg.device_key => Ok(()),
        _ => Err(ApiError::unauthorized("X-Device-Key salah atau tidak ada")),
    }
}

/// Aksi kontrol dari dashboard (mulai rekam, OTA, QC) wajib membawa `X-Operator-Key`.
pub fn require_operator(h: &HeaderMap, st: &AppState) -> Result<(), ApiError> {
    match header(h, "x-operator-key") {
        Some(k) if k == st.cfg.operator_key => Ok(()),
        _ => Err(ApiError::unauthorized("Kunci operator diperlukan untuk aksi ini")),
    }
}
