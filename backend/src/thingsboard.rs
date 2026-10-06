//! Klien REST ThingsBoard untuk manajemen OTA firmware.
//!
//! Alur resmi ThingsBoard OTA:
//!   1. POST /api/otaPackage                        → buat info paket (title, version, profil device)
//!   2. POST /api/otaPackage/{id}?checksumAlgorithm → unggah file .bin (multipart "file")
//!   3. POST /api/device (firmwareId = paket)       → assign ke device
//!   4. ThingsBoard men-set shared attribute fw_title/fw_version/fw_checksum pada device;
//!      ESP32 membaca atribut itu, mengunduh /api/v1/{token}/firmware, lalu melaporkan
//!      telemetry fw_state (DOWNLOADING → DOWNLOADED → VERIFIED → UPDATING → UPDATED/FAILED).

use crate::config::TbConfig;
use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::Mutex;

pub struct ThingsBoard {
    cfg: TbConfig,
    http: reqwest::Client,
    jwt: Mutex<Option<String>>,
}

#[derive(Debug, Serialize)]
pub struct TbDevice {
    pub id: String,
    pub name: String,
    pub device_profile_id: Option<String>,
    pub firmware_id: Option<String>,
    #[serde(skip)]
    pub raw: Value,
}

#[derive(Debug, Serialize)]
pub struct TbPackage {
    pub id: String,
    pub title: String,
    pub version: String,
    pub file_name: Option<String>,
    pub data_size: Option<i64>,
    pub checksum: Option<String>,
    pub has_data: bool,
    pub created_time: Option<i64>,
    pub device_profile_id: Option<String>,
}

fn id_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.get("id")).and_then(|x| x.as_str()).map(str::to_string)
}

impl TbPackage {
    fn from_json(v: &Value) -> Self {
        Self {
            id: id_of(v, "id").unwrap_or_default(),
            title: v["title"].as_str().unwrap_or_default().into(),
            version: v["version"].as_str().unwrap_or_default().into(),
            file_name: v["fileName"].as_str().map(str::to_string),
            data_size: v["dataSize"].as_i64(),
            checksum: v["checksum"].as_str().map(str::to_string),
            has_data: v["hasData"].as_bool().unwrap_or(false),
            created_time: v["createdTime"].as_i64(),
            device_profile_id: id_of(v, "deviceProfileId"),
        }
    }
}

impl ThingsBoard {
    pub fn new(cfg: TbConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("reqwest client");
        Self { cfg, http, jwt: Mutex::new(None) }
    }

    pub fn url(&self) -> &str {
        &self.cfg.url
    }

    async fn auth_header(&self, force_login: bool) -> Result<String> {
        if let Some(key) = &self.cfg.api_key {
            return Ok(format!("ApiKey {key}"));
        }
        let mut jwt = self.jwt.lock().await;
        if jwt.is_none() || force_login {
            let (Some(u), Some(p)) = (&self.cfg.username, &self.cfg.password) else {
                bail!("TB_USERNAME/TB_PASSWORD atau TB_API_KEY belum diset");
            };
            let resp = self
                .http
                .post(format!("{}/api/auth/login", self.cfg.url))
                .json(&json!({ "username": u, "password": p }))
                .send()
                .await
                .context("Tidak bisa menghubungi ThingsBoard")?;
            if !resp.status().is_success() {
                bail!("Login ThingsBoard ditolak ({})", resp.status());
            }
            let v: Value = resp.json().await?;
            *jwt = Some(v["token"].as_str().ok_or_else(|| anyhow!("token login kosong"))?.to_string());
        }
        Ok(format!("Bearer {}", jwt.as_ref().unwrap()))
    }

    /// Kirim request; login ulang otomatis sekali bila JWT kedaluwarsa (401).
    async fn send(&self, build: impl Fn(&reqwest::Client) -> reqwest::RequestBuilder) -> Result<Value> {
        for attempt in 0..2 {
            let auth = self.auth_header(attempt == 1).await?;
            let resp = build(&self.http).header("X-Authorization", auth).send().await?;
            let status = resp.status();
            if status == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 && self.cfg.api_key.is_none() {
                continue;
            }
            let text = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                let msg = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v["message"].as_str().map(str::to_string))
                    .unwrap_or(text);
                bail!("ThingsBoard {status}: {msg}");
            }
            return Ok(if text.is_empty() { Value::Null } else { serde_json::from_str(&text)? });
        }
        bail!("Autentikasi ThingsBoard gagal")
    }

    pub async fn check(&self) -> Result<Value> {
        let base = self.cfg.url.clone();
        self.send(|c| c.get(format!("{base}/api/auth/user"))).await
    }

    pub async fn find_device(&self, name: &str) -> Result<TbDevice> {
        let base = self.cfg.url.clone();
        let name_q = name.to_string();
        let v = self
            .send(|c| c.get(format!("{base}/api/tenant/devices")).query(&[("deviceName", name_q.as_str())]))
            .await
            .with_context(|| format!("Device '{name}' tidak ditemukan di ThingsBoard (nama device harus sama dengan DEVICE_ID)"))?;
        Ok(TbDevice {
            id: id_of(&v, "id").ok_or_else(|| anyhow!("respons device tanpa id"))?,
            name: v["name"].as_str().unwrap_or(name).into(),
            device_profile_id: id_of(&v, "deviceProfileId"),
            firmware_id: id_of(&v, "firmwareId"),
            raw: v,
        })
    }

    pub async fn list_packages(&self) -> Result<Vec<TbPackage>> {
        let base = self.cfg.url.clone();
        let v = self
            .send(|c| {
                c.get(format!("{base}/api/otaPackages")).query(&[
                    ("pageSize", "50"),
                    ("page", "0"),
                    ("sortProperty", "createdTime"),
                    ("sortOrder", "DESC"),
                ])
            })
            .await?;
        Ok(v["data"].as_array().map(|a| a.iter().map(TbPackage::from_json).collect()).unwrap_or_default())
    }

    /// Buat paket OTA + unggah biner. Mengembalikan paket yang sudah berisi data.
    pub async fn upload_package(
        &self,
        title: &str,
        version: &str,
        device_profile_id: &str,
        file_name: &str,
        data: Vec<u8>,
    ) -> Result<TbPackage> {
        let base = self.cfg.url.clone();
        let body = json!({
            "title": title,
            "version": version,
            "tag": format!("{title} {version}"),
            "type": "FIRMWARE",
            "deviceProfileId": { "id": device_profile_id, "entityType": "DEVICE_PROFILE" },
            "isURL": false,
        });
        let info = self.send(|c| c.post(format!("{base}/api/otaPackage")).json(&body)).await.context("Gagal membuat paket OTA")?;
        let id = id_of(&info, "id").ok_or_else(|| anyhow!("paket OTA tanpa id"))?;
        let fname = file_name.to_string();
        let v = self
            .send(|c| {
                let part = reqwest::multipart::Part::bytes(data.clone())
                    .file_name(fname.clone())
                    .mime_str("application/octet-stream")
                    .unwrap();
                c.post(format!("{base}/api/otaPackage/{id}"))
                    .query(&[("checksumAlgorithm", "SHA256")])
                    .multipart(reqwest::multipart::Form::new().part("file", part))
            })
            .await
            .context("Gagal mengunggah file firmware ke ThingsBoard")?;
        Ok(TbPackage::from_json(&v))
    }

    /// Assign paket firmware ke device → ThingsBoard men-set shared attributes fw_*.
    pub async fn assign(&self, device: &TbDevice, package_id: &str) -> Result<()> {
        let base = self.cfg.url.clone();
        let mut d = device.raw.clone();
        d["firmwareId"] = json!({ "id": package_id, "entityType": "OTA_PACKAGE" });
        self.send(|c| c.post(format!("{base}/api/device")).json(&d)).await.context("Gagal assign firmware ke device")?;
        Ok(())
    }

    /// JWT baru (login ulang) untuk WebSocket telemetry — hanya dengan TB_USERNAME/TB_PASSWORD.
    pub async fn fresh_jwt(&self) -> Result<String> {
        if self.cfg.username.is_none() || self.cfg.password.is_none() {
            bail!("Jembatan MQTT butuh TB_USERNAME dan TB_PASSWORD (TB_API_KEY saja tidak cukup untuk WebSocket)");
        }
        let auth = self.auth_header(true).await?;
        Ok(auth.trim_start_matches("Bearer ").to_string())
    }

    /// URL WebSocket telemetry (wss://host/api/ws).
    pub fn ws_url(&self) -> String {
        let base = self.cfg.url.replacen("https://", "wss://", 1).replacen("http://", "ws://", 1);
        format!("{base}/api/ws")
    }

    /// Semua device milik tenant: (id ThingsBoard, nama).
    pub async fn list_devices(&self) -> Result<Vec<(String, String)>> {
        let base = self.cfg.url.clone();
        let v = self
            .send(|c| c.get(format!("{base}/api/tenant/devices")).query(&[("pageSize", "200"), ("page", "0")]))
            .await?;
        Ok(v["data"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|d| Some((id_of(d, "id")?, d["name"].as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// RPC satu arah server → device (diterima ESP32 di v1/devices/me/rpc/request/+).
    pub async fn send_rpc(&self, tb_device_id: &str, method: &str, params: Value) -> Result<()> {
        let base = self.cfg.url.clone();
        let id = tb_device_id.to_string();
        let body = json!({ "method": method, "params": params, "timeout": 5000 });
        self.send(|c| c.post(format!("{base}/api/rpc/oneway/{id}")).json(&body)).await?;
        Ok(())
    }

    /// Riwayat telemetry (urut naik) — untuk mengejar pesan yang terlewat saat WebSocket putus.
    pub async fn history(&self, tb_device_id: &str, keys: &str, start_ts: i64, end_ts: i64) -> Result<Vec<(String, i64, String)>> {
        let base = self.cfg.url.clone();
        let id = tb_device_id.to_string();
        let (keys, start, end) = (keys.to_string(), start_ts.to_string(), end_ts.to_string());
        let v = self
            .send(|c| {
                c.get(format!("{base}/api/plugins/telemetry/DEVICE/{id}/values/timeseries")).query(&[
                    ("keys", keys.as_str()),
                    ("startTs", start.as_str()),
                    ("endTs", end.as_str()),
                    ("limit", "1000"),
                    ("orderBy", "ASC"),
                    ("agg", "NONE"),
                ])
            })
            .await?;
        let mut out = Vec::new();
        if let Some(obj) = v.as_object() {
            for (k, arr) in obj {
                for p in arr.as_array().into_iter().flatten() {
                    if let (Some(ts), Some(val)) = (p["ts"].as_i64(), p["value"].as_str()) {
                        out.push((k.clone(), ts, val.to_string()));
                    }
                }
            }
        }
        out.sort_by_key(|x| x.1);
        Ok(out)
    }

    /// Telemetry status OTA terbaru yang dilaporkan ESP32 ke ThingsBoard.
    pub async fn fw_telemetry(&self, tb_device_id: &str) -> Result<Value> {
        let base = self.cfg.url.clone();
        let id = tb_device_id.to_string();
        let v = self
            .send(|c| {
                c.get(format!("{base}/api/plugins/telemetry/DEVICE/{id}/values/timeseries"))
                    .query(&[("keys", "fw_state,fw_error,current_fw_title,current_fw_version,target_fw_version")])
            })
            .await?;
        let mut out = serde_json::Map::new();
        if let Some(obj) = v.as_object() {
            for (k, arr) in obj {
                if let Some(first) = arr.as_array().and_then(|a| a.first()) {
                    out.insert(k.clone(), json!({ "value": first["value"], "ts": first["ts"] }));
                }
            }
        }
        Ok(Value::Object(out))
    }
}
