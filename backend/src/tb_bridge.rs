//! Jembatan ThingsBoard ↔ backend (komunikasi ESP32 lewat MQTT).
//!
//! ESP32 tidak memanggil backend langsung; semua lewat MQTT ThingsBoard:
//!   • ESP32 publish telemetry enose_hb / enose_samples / enose_event / enose_reading
//!     → dibaca di sini lewat WebSocket ThingsBoard → diproses sama seperti endpoint HTTP.
//!   • Perintah dashboard (pending_command) → RPC ThingsBoard "enose_cmd" → ESP32,
//!     dikirim ulang tiap 4 detik sampai id-nya muncul sebagai last_ack di heartbeat.
//!
//! Butuh TB_URL + TB_USERNAME + TB_PASSWORD (akun tenant ThingsBoard).

use crate::models::*;
use crate::routes::device;
use crate::state::Shared;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

/// Kunci telemetry — harus sama dengan firmware (mqtt.rs).
const KEY_HEARTBEAT: &str = "enose_hb";
const KEY_SAMPLES: &str = "enose_samples";
const KEY_EVENT: &str = "enose_event";
const KEY_READING: &str = "enose_reading";
const KEYS: &str = "enose_hb,enose_samples,enose_event,enose_reading";

const RPC_METHOD: &str = "enose_cmd";
const RPC_RESEND: Duration = Duration::from_secs(4);
/// Heartbeat lebih tua dari ini (mis. hasil mengejar riwayat) tidak dianggap tanda online.
const HB_MAX_AGE_MS: i64 = 15_000;
const RETRY_MAX: u32 = 20;

/// device_id firmware (DAQ01) → id device ThingsBoard.
type Routes = Arc<RwLock<HashMap<String, String>>>;

/// Pesan yang gagal diproses (mis. database sedang putus) — dicoba lagi berkala.
struct Retry {
    tb_id: String,
    key: String,
    value: String,
    attempts: u32,
}

pub fn start(st: Shared) {
    if st.tb.is_none() {
        tracing::warn!("TB_URL belum diset — jembatan MQTT ThingsBoard nonaktif, ESP32 tidak akan terlihat");
        return;
    }
    let routes = Routes::default();
    tokio::spawn(telemetry_loop(st.clone(), routes.clone()));
    tokio::spawn(command_loop(st, routes));
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

// ─────────────────────────── ThingsBoard → backend ───────────────────────────

struct Ingest {
    st: Shared,
    routes: Routes,
    /// (id device TB, kunci) → ts terakhir yang sudah diproses (dedup + titik awal kejar riwayat).
    last_ts: HashMap<(String, String), i64>,
    start_ms: i64,
    retry: Vec<Retry>,
}

async fn telemetry_loop(st: Shared, routes: Routes) {
    let mut ing = Ingest { st, routes, last_ts: HashMap::new(), start_ms: now_ms(), retry: Vec::new() };
    let mut backoff = 2u64;
    loop {
        let t0 = Instant::now();
        match session(&mut ing).await {
            Ok(()) => tracing::warn!("WebSocket ThingsBoard ditutup — menyambung ulang"),
            Err(e) => tracing::error!("Jembatan MQTT ThingsBoard: {e:#}"),
        }
        if t0.elapsed() > Duration::from_secs(120) {
            backoff = 2;
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(60);
    }
}

async fn session(ing: &mut Ingest) -> Result<()> {
    let st = ing.st.clone();
    let tb = st.tb.as_ref().unwrap();
    let jwt = tb.fresh_jwt().await?;
    let devices = tb.list_devices().await.context("Gagal membaca daftar device ThingsBoard")?;
    if devices.is_empty() {
        return Err(anyhow!("belum ada device di ThingsBoard (buat device bernama DAQ01 dst.)"));
    }
    {
        let mut r = ing.routes.write().await;
        for (id, name) in &devices {
            r.entry(name.clone()).or_insert_with(|| id.clone());
        }
    }

    let (mut ws, _) = tokio_tungstenite::connect_async(tb.ws_url()).await.context("WebSocket ThingsBoard")?;
    let mut subs: HashMap<i64, String> = HashMap::new();
    let cmds: Vec<Value> = devices
        .iter()
        .enumerate()
        .map(|(i, (id, _))| {
            let cmd_id = i as i64 + 1;
            subs.insert(cmd_id, id.clone());
            json!({ "type": "TIMESERIES", "entityType": "DEVICE", "entityId": id,
                    "scope": "LATEST_TELEMETRY", "cmdId": cmd_id, "keys": KEYS })
        })
        .collect();
    ws.send(Message::Text(json!({ "authCmd": { "cmdId": 0, "token": jwt }, "cmds": cmds }).to_string().into()))
        .await?;
    let names: Vec<&str> = devices.iter().map(|(_, n)| n.as_str()).collect();
    tracing::info!("Jembatan MQTT: berlangganan telemetry ThingsBoard untuk {}", names.join(", "));

    // Kejar pesan yang masuk selama WebSocket putus (pesan WS baru tertahan di socket sementara itu).
    let now = now_ms();
    for (id, _) in &devices {
        let from = KEYS
            .split(',')
            .map(|k| ing.last_ts.get(&(id.clone(), k.to_string())).copied().unwrap_or(ing.start_ms))
            .min()
            .unwrap_or(ing.start_ms);
        if now - from > 1000 {
            match tb.history(id, KEYS, from + 1, now).await {
                Ok(items) => {
                    for (k, ts, v) in items {
                        ing.handle(id, &k, ts, v).await;
                    }
                }
                Err(e) => tracing::warn!("Gagal mengejar riwayat telemetry {id}: {e:#}"),
            }
        }
    }

    let known: HashSet<String> = devices.into_iter().map(|(id, _)| id).collect();
    let mut ping = tokio::time::interval(Duration::from_secs(30));
    let mut refresh = tokio::time::interval(Duration::from_secs(300));
    let mut retry = tokio::time::interval(Duration::from_secs(30));
    refresh.tick().await;
    retry.tick().await;
    loop {
        tokio::select! {
            msg = ws.next() => {
                let Some(msg) = msg else { return Ok(()) };
                match msg? {
                    Message::Text(t) => ing.on_ws_text(&t, &subs).await,
                    Message::Close(_) => return Ok(()),
                    _ => {}
                }
            }
            _ = ping.tick() => ws.send(Message::Ping(Vec::new().into())).await?,
            _ = retry.tick() => ing.run_retries().await,
            _ = refresh.tick() => {
                // Device baru ditambahkan di ThingsBoard → sambung ulang agar ikut berlangganan.
                if let Ok(list) = tb.list_devices().await {
                    if list.iter().any(|(id, _)| !known.contains(id)) {
                        tracing::info!("Device ThingsBoard baru terdeteksi — memperbarui langganan");
                        return Ok(());
                    }
                }
            }
        }
    }
}

impl Ingest {
    async fn on_ws_text(&mut self, text: &str, subs: &HashMap<i64, String>) {
        let Ok(v) = serde_json::from_str::<Value>(text) else { return };
        if v["errorCode"].as_i64().unwrap_or(0) != 0 {
            tracing::warn!("ThingsBoard WebSocket error: {}", v["errorMsg"]);
            return;
        }
        let Some(tb_id) = v["subscriptionId"].as_i64().and_then(|s| subs.get(&s)) else { return };
        let Some(data) = v["data"].as_object() else { return };
        let mut items: Vec<(String, i64, String)> = Vec::new();
        for (k, arr) in data {
            for p in arr.as_array().into_iter().flatten() {
                if let (Some(ts), Some(val)) = (p[0].as_i64(), p[1].as_str()) {
                    items.push((k.clone(), ts, val.to_string()));
                }
            }
        }
        items.sort_by_key(|x| x.1);
        for (k, ts, val) in items {
            self.handle(tb_id, &k, ts, val).await;
        }
    }

    async fn handle(&mut self, tb_id: &str, key: &str, ts: i64, value: String) {
        let slot = self.last_ts.entry((tb_id.to_string(), key.to_string())).or_insert(self.start_ms);
        if ts <= *slot {
            return; // sudah diproses / nilai lama saat berlangganan
        }
        *slot = ts;
        if key == KEY_HEARTBEAT && now_ms() - ts > HB_MAX_AGE_MS {
            return;
        }
        if let Err(e) = process(&self.st, &self.routes, tb_id, key, &value).await {
            tracing::warn!("Telemetry {key} dari device TB {tb_id} gagal diproses (dicoba lagi): {e:#}");
            if key != KEY_HEARTBEAT {
                self.retry.push(Retry { tb_id: tb_id.into(), key: key.into(), value, attempts: 1 });
            }
        }
    }

    async fn run_retries(&mut self) {
        for mut r in std::mem::take(&mut self.retry) {
            match process(&self.st, &self.routes, &r.tb_id, &r.key, &r.value).await {
                Ok(()) => tracing::info!("Telemetry {} tertunda berhasil diproses", r.key),
                Err(e) if r.attempts >= RETRY_MAX => {
                    tracing::error!("Telemetry {} dibuang setelah {} percobaan: {e:#}", r.key, r.attempts)
                }
                Err(_) => {
                    r.attempts += 1;
                    self.retry.push(r);
                }
            }
        }
    }
}

async fn process(st: &Shared, routes: &Routes, tb_id: &str, key: &str, value: &str) -> Result<()> {
    let api = |e: crate::error::ApiError| anyhow!("{} {}", e.status, e.message);
    match key {
        KEY_HEARTBEAT => {
            let hb: Heartbeat = serde_json::from_str(value)?;
            routes.write().await.insert(hb.device_id.clone(), tb_id.to_string());
            device::process_heartbeat(st, hb).await.map_err(api)?;
        }
        KEY_SAMPLES => {
            device::process_samples(st, serde_json::from_str(value)?).await.map_err(api)?;
        }
        KEY_EVENT => {
            device::process_event(st, serde_json::from_str(value)?).await.map_err(api)?;
        }
        KEY_READING => {
            device::process_reading(st, serde_json::from_str(value)?).await.map_err(api)?;
        }
        _ => {}
    }
    Ok(())
}

// ─────────────────────────── backend → ESP32 (RPC) ───────────────────────────

async fn command_loop(st: Shared, routes: Routes) {
    let mut sent: HashMap<String, (u64, Instant)> = HashMap::new();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tick.tick().await;
        let pending: Vec<(String, Command)> = st
            .devices
            .read()
            .await
            .iter()
            .filter(|(_, d)| d.online())
            .filter_map(|(k, d)| d.pending_command.clone().map(|c| (k.clone(), c)))
            .collect();
        sent.retain(|dev, _| pending.iter().any(|(d, _)| d == dev));
        for (dev, cmd) in pending {
            if let Some((id, at)) = sent.get(&dev) {
                if *id == cmd.id && at.elapsed() < RPC_RESEND {
                    continue;
                }
            }
            let Some(tb_id) = routes.read().await.get(&dev).cloned() else {
                tracing::warn!("Perintah untuk {dev} tertahan: device ThingsBoard-nya belum diketahui");
                continue;
            };
            let first = sent.get(&dev).map(|(id, _)| *id != cmd.id).unwrap_or(true);
            sent.insert(dev.clone(), (cmd.id, Instant::now()));
            let st = st.clone();
            tokio::spawn(async move {
                let params = serde_json::to_value(&cmd).unwrap_or_default();
                match st.tb.as_ref().unwrap().send_rpc(&tb_id, RPC_METHOD, params).await {
                    Ok(()) if first => tracing::info!("RPC '{}' (id {}) dikirim ke {dev}", cmd.kind, cmd.id),
                    Ok(()) => {}
                    Err(e) => tracing::warn!("RPC '{}' ke {dev} gagal (dicoba lagi): {e:#}", cmd.kind),
                }
            });
        }
    }
}
