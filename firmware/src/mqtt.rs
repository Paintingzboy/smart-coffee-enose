//! Koneksi MQTT ke ThingsBoard (Device MQTT API).
//!
//!   • Kirim  : v1/devices/me/telemetry          {"enose_hb": "<json>"} dst.
//!   • Terima : v1/devices/me/rpc/request/{id}   {"method":"enose_cmd","params":{...}}
//!
//! Login memakai access token device ThingsBoard sebagai username MQTT. Backend
//! Railway membaca telemetry ini dari ThingsBoard dan mengirim perintah lewat RPC.

use crate::config::*;
use crate::shared::ServerCommand;
use anyhow::{anyhow, bail, Result};
use esp_idf_svc::mqtt::client::{
    Details, EspMqttClient, EventPayload, MqttClientConfiguration, MqttProtocolVersion, QoS,
};
use log::*;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TOPIC_TELEMETRY: &str = "v1/devices/me/telemetry";
const TOPIC_RPC_SUB: &str = "v1/devices/me/rpc/request/+";
const TOPIC_RPC_PREFIX: &str = "v1/devices/me/rpc/request/";
const RPC_METHOD: &str = "enose_cmd";

/// Kunci telemetry — harus sama dengan backend (tb_bridge.rs).
pub const KEY_HEARTBEAT: &str = "enose_hb";
pub const KEY_SAMPLES: &str = "enose_samples";
pub const KEY_EVENT: &str = "enose_event";
pub const KEY_READING: &str = "enose_reading";

#[derive(Default)]
struct Inner {
    connected: AtomicBool,
    need_subscribe: AtomicBool,
    /// ID pesan QoS 1 yang sudah di-PUBACK broker.
    acked: Mutex<VecDeque<u32>>,
    commands: Mutex<VecDeque<ServerCommand>>,
}

pub struct Mqtt {
    client: EspMqttClient<'static>,
    inner: Arc<Inner>,
}

impl Mqtt {
    pub fn new() -> Result<Self> {
        if TB_TOKEN.is_empty() {
            bail!("TB_TOKEN kosong — isi access token device ThingsBoard di config.rs");
        }
        let inner = Arc::new(Inner::default());
        let cb_inner = inner.clone();
        let conf = MqttClientConfiguration {
            protocol_version: Some(MqttProtocolVersion::V3_1_1),
            client_id: Some(DEVICE_ID),
            username: Some(TB_TOKEN),
            keep_alive_interval: Some(Duration::from_secs(30)),
            reconnect_timeout: Some(Duration::from_secs(5)),
            network_timeout: Duration::from_secs(15),
            buffer_size: 4096,
            out_buffer_size: 4096,
            task_stack: 8192,
            crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
            ..Default::default()
        };
        let client = EspMqttClient::new_cb(TB_MQTT_URL, &conf, move |ev| match ev.payload() {
            EventPayload::Connected(_) => {
                info!("MQTT tersambung ke {TB_MQTT_URL}");
                cb_inner.connected.store(true, Ordering::SeqCst);
                cb_inner.need_subscribe.store(true, Ordering::SeqCst);
            }
            EventPayload::Disconnected => {
                warn!("MQTT terputus — menyambung ulang otomatis");
                cb_inner.connected.store(false, Ordering::SeqCst);
            }
            EventPayload::Published(id) => {
                let mut a = cb_inner.acked.lock().unwrap();
                a.push_back(id);
                while a.len() > 64 {
                    a.pop_front();
                }
            }
            EventPayload::Received { topic, data, details, .. } => {
                if !matches!(details, Details::Complete) {
                    warn!("MQTT: pesan terpecah diabaikan ({} byte)", data.len());
                    return;
                }
                if topic.map(|t| t.starts_with(TOPIC_RPC_PREFIX)).unwrap_or(false) {
                    match parse_rpc(data) {
                        Ok(cmd) => cb_inner.commands.lock().unwrap().push_back(cmd),
                        Err(e) => warn!("RPC tidak valid: {e:#}"),
                    }
                }
            }
            EventPayload::Error(e) => warn!("MQTT error: {e:?}"),
            _ => {}
        })?;
        Ok(Self { client, inner })
    }

    pub fn is_connected(&self) -> bool {
        self.inner.connected.load(Ordering::SeqCst)
    }

    /// Dipanggil tiap putaran loop: berlangganan RPC setelah (re)connect.
    pub fn poll(&mut self) {
        if self.is_connected() && self.inner.need_subscribe.swap(false, Ordering::SeqCst) {
            if let Err(e) = self.client.subscribe(TOPIC_RPC_SUB, QoS::AtLeastOnce) {
                warn!("Subscribe RPC gagal: {e}");
                self.inner.need_subscribe.store(true, Ordering::SeqCst);
            }
        }
    }

    pub fn take_command(&self) -> Option<ServerCommand> {
        self.inner.commands.lock().unwrap().pop_front()
    }

    /// Publish telemetry `{key: body}` (QoS 1). Bila `wait`, tunggu PUBACK dari broker
    /// sehingga pemanggil boleh menghapus data dari antrean.
    pub fn send(&mut self, key: &str, body: &str, wait: bool) -> Result<()> {
        if !self.is_connected() {
            bail!("MQTT belum tersambung");
        }
        let payload = json!({ key: body }).to_string();
        let id = self.client.publish(TOPIC_TELEMETRY, QoS::AtLeastOnce, false, payload.as_bytes())?;
        if !wait {
            return Ok(());
        }
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_secs(10) {
            if self.inner.acked.lock().unwrap().contains(&id) {
                return Ok(());
            }
            if !self.is_connected() {
                bail!("MQTT terputus sebelum PUBACK");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        bail!("PUBACK tidak diterima dalam 10 detik")
    }
}

fn parse_rpc(data: &[u8]) -> Result<ServerCommand> {
    let v: Value = serde_json::from_slice(data)?;
    if v["method"].as_str() != Some(RPC_METHOD) {
        return Err(anyhow!("method tidak dikenal: {}", v["method"]));
    }
    Ok(serde_json::from_value(v["params"].clone())?)
}
