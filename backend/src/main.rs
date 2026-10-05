//! Smart Coffee E-Nose — Backend (Rust + Axum)
//!
//! Satu proses melayani:
//!   • API perangkat  (/api/device/*, /api/readings)  ← ESP32-S3 via HTTPS
//!   • API dashboard  (/api/*)                         ← browser
//!   • File statis dashboard React (frontend/dist)     ← satu URL publik

mod config;
mod error;
mod models;
mod routes;
mod state;
mod store;
mod thingsboard;

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, patch, post},
    Router,
};
use routes::{dashboard, device, export, ota};
use state::AppState;
use std::{net::SocketAddr, path::Path, sync::Arc};
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "enose_backend=info,tower_http=warn".into()),
        )
        .init();

    let cfg = config::Config::from_env();
    let store = match &cfg.db {
        Some(db) => {
            tracing::info!("Database: Azure SQL {}/{}", db.host, db.database);
            store::Store::Sql(store::SqlStore::new(db.clone()))
        }
        None => {
            tracing::warn!("DATABASE_* belum diset — data disimpan di MEMORI dan hilang saat restart");
            store::Store::Mem(store::MemStore::default())
        }
    };
    if let Err(e) = store.migrate().await {
        // Jangan matikan server: dashboard tetap bisa menampilkan status error DB.
        tracing::error!("Migrasi database gagal: {e:#}");
    }
    let tb = cfg.tb.clone().map(thingsboard::ThingsBoard::new);
    let port = cfg.port;
    let static_dir = cfg.static_dir.clone();
    let st = Arc::new(AppState::new(cfg, store, tb));

    dashboard::restore_active(&st).await;
    tokio::spawn(dashboard::sweeper(st.clone()));

    let api = Router::new()
        // ── perangkat (X-Device-Key) ──
        .route("/device/heartbeat", post(device::heartbeat))
        .route("/device/samples", post(device::samples))
        .route("/device/event", post(device::event))
        .route("/readings", post(device::reading))
        // ── dashboard: baca ──
        .route("/health", get(|| async { "ok" }))
        .route("/status", get(dashboard::status))
        .route("/model", get(dashboard::model))
        .route("/devices", get(dashboard::devices))
        .route("/devices/{id}/live", get(dashboard::device_live))
        .route("/measurements", get(dashboard::list_measurements))
        .route("/measurements/next-id", get(dashboard::next_id))
        .route("/measurements/{id}", get(dashboard::get_measurement))
        .route("/measurements/{id}/samples", get(dashboard::measurement_samples))
        .route("/measurements/{id}/raw.csv", get(export::measurement_csv))
        .route("/stats", get(dashboard::stats))
        .route("/features", get(dashboard::features))
        .route("/export/metadata.csv", get(export::export_metadata))
        .route("/export/measurements.json", get(export::export_json))
        .route("/export/dataset.zip", get(export::export_zip))
        .route("/ota/packages", get(ota::packages))
        .route("/ota/devices/{id}", get(ota::device_status))
        // ── dashboard: kontrol (X-Operator-Key) ──
        .route("/auth/check", post(dashboard::auth_check))
        .route("/measurements/start", post(dashboard::start_measurement))
        .route("/measurements/{id}/qc", patch(dashboard::set_qc))
        .route("/devices/{id}/commands", post(dashboard::device_command))
        .route("/ota/assign", post(ota::assign))
        .route("/ota/upload", post(ota::upload).layer(DefaultBodyLimit::max(8 * 1024 * 1024)));

    let index = Path::new(&static_dir).join("index.html");
    if !index.exists() {
        tracing::warn!("{} tidak ditemukan — jalankan `npm run build` di frontend", index.display());
    }
    let app = Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new(&static_dir).not_found_service(ServeFile::new(index)))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(st);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Smart Coffee E-Nose backend di http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
