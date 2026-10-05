use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }
    pub fn bad_request(m: impl Into<String>) -> Self { Self::new(StatusCode::BAD_REQUEST, m) }
    pub fn unauthorized(m: impl Into<String>) -> Self { Self::new(StatusCode::UNAUTHORIZED, m) }
    pub fn not_found(m: impl Into<String>) -> Self { Self::new(StatusCode::NOT_FOUND, m) }
    pub fn conflict(m: impl Into<String>) -> Self { Self::new(StatusCode::CONFLICT, m) }
    pub fn unavailable(m: impl Into<String>) -> Self { Self::new(StatusCode::SERVICE_UNAVAILABLE, m) }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!("{e:#}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
