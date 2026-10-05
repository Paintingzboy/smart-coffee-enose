//! Konfigurasi dari environment variable (.env lokal / Variables di Railway).

use std::env;

#[derive(Clone, Debug)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

#[derive(Clone, Debug)]
pub struct TbConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub version: String,
    /// Contoh: "Macro-F1 0,81 ± 0,07 (GroupKFold k=6, grup=Coffee_ID)"
    pub metric: Option<String>,
    pub validation: Option<String>,
    pub confidence_threshold: f64,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub static_dir: String,
    pub db: Option<DbConfig>,
    pub device_key: String,
    pub operator_key: String,
    pub tb: Option<TbConfig>,
    pub fw_title: String,
    pub model: ModelInfo,
}

fn var(name: &str) -> Option<String> {
    env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

impl Config {
    pub fn from_env() -> Self {
        let db = match (var("DATABASE_HOST"), var("DATABASE_USER"), var("DATABASE_PASSWORD"), var("DATABASE_NAME")) {
            (Some(host), Some(user), Some(password), Some(database)) => Some(DbConfig {
                host,
                port: var("DATABASE_PORT").and_then(|p| p.parse().ok()).unwrap_or(1433),
                user,
                password,
                database,
            }),
            _ => None,
        };
        let tb = var("TB_URL").map(|url| TbConfig {
            url: url.trim_end_matches('/').to_string(),
            username: var("TB_USERNAME"),
            password: var("TB_PASSWORD"),
            api_key: var("TB_API_KEY"),
        });
        let device_key = var("DEVICE_API_KEY").unwrap_or_else(|| {
            tracing::warn!("DEVICE_API_KEY belum diset — memakai default 'dev-device-key' (JANGAN untuk publik)");
            "dev-device-key".into()
        });
        let operator_key = var("OPERATOR_KEY").unwrap_or_else(|| {
            tracing::warn!("OPERATOR_KEY belum diset — memakai default 'dev-operator' (JANGAN untuk publik)");
            "dev-operator".into()
        });
        Self {
            port: var("PORT").and_then(|p| p.parse().ok()).unwrap_or(3000),
            static_dir: var("STATIC_DIR").unwrap_or_else(|| "../frontend/dist".into()),
            db,
            device_key,
            operator_key,
            tb,
            fw_title: var("FW_TITLE").unwrap_or_else(|| "smart-coffee-enose".into()),
            model: ModelInfo {
                name: var("MODEL_NAME").unwrap_or_else(|| "Edge Impulse (belum terhubung)".into()),
                version: var("MODEL_VERSION").unwrap_or_else(|| "none".into()),
                metric: var("MODEL_METRIC"),
                validation: var("MODEL_VALIDATION"),
                confidence_threshold: var("CONFIDENCE_THRESHOLD").and_then(|v| v.parse().ok()).unwrap_or(0.6),
            },
        }
    }
}
