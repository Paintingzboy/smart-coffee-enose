# Smart Coffee E-Nose — IoT Electronic Nose + TinyML + Dashboard

Case-Based Project **Teknologi IoT 5A** — Departemen Teknik Instrumentasi, Fakultas Vokasi ITS.
Dosen pengampu: Ahmad Radhy.

```
ESP32-S3 (Rust) ══MQTT/TLS══► ThingsBoard Cloud EU ◄══WebSocket + REST══ Backend Rust/Axum (Railway) ──► Azure SQL
   telemetry: heartbeat 5 s,       │  (broker MQTT)          RPC perintah      │  juga menyajikan dashboard React
   data mentah, event, hasil   ◄───┘  RPC: start/stop/…                         │
   OTA (HTTP Device API)       ◄────── paket .bin ◄──────── dashboard (unggah .bin)
```

Semua komunikasi ESP32 ↔ server lewat **MQTT** ThingsBoard (`mqtt.eu.thingsboard.cloud:8883`):
ESP32 publish telemetry `enose_hb`, `enose_samples`, `enose_event`, `enose_reading` ke
`v1/devices/me/telemetry` dan menerima perintah dashboard sebagai RPC `enose_cmd`. Backend
(`backend/src/tb_bridge.rs`) membaca telemetry itu lewat WebSocket ThingsBoard dan mengirim RPC.

ESP32 hanya membuat koneksi **keluar** (MQTT/TLS) ke server publik. Karena itu perangkat bisa
dikendalikan dari mana saja (PC/HP) tanpa IP publik: cukup colokkan ESP32 ke daya → WiFi
tersambung → muncul **online** di dashboard → operator menekan **Mulai rekam** →
baca sensor 500 s → fitur + TinyML → simpan ke database.

## Isi repositori

| Folder | Isi |
|---|---|
| `backend/` | Server Rust (Axum + Tiberius). API perangkat, API dashboard, ekspor dataset, klien ThingsBoard OTA. Tabel Azure SQL dibuat otomatis. |
| `frontend/` | Dashboard React + Tailwind + Recharts (5 halaman sesuai wireframe). |
| `firmware/` | Firmware ESP32-S3 Rust (pengganti `teknologi-iot-a`). |
| `tools/simulate_device.py` | Simulator ESP32 untuk menguji dashboard tanpa hardware. |
| `docs/DEPLOY.md` | Langkah deploy Railway + Azure SQL + ThingsBoard, langkah demi langkah. |
| `docs/FIRMWARE.md` | Build, flash, OTA, Edge Impulse. |
| `Dockerfile`, `railway.json` | Build satu image (frontend + backend) untuk Railway. |

## Status verifikasi

| Bagian | Diverifikasi |
|---|---|
| Backend | Kompilasi tanpa warning; diuji end-to-end dengan simulator (rekam, stop, QC, ID ganda, ekspor CSV/metadata/ZIP). Koneksi ke Azure SQL asli belum diuji. |
| Dashboard | Build produksi sukses; dicek di desktop 1366 px dan mobile 390 px. |
| Firmware | Kompilasi & link penuh dengan ESP-IDF v5.5.3 (0 error, 0 warning), ±1,27 MB. Belum diuji di board fisik. |
| Docker/Railway | Dockerfile ditulis mengikuti build yang sama; belum dijalankan di Railway. |

## Menjalankan lokal (tanpa hardware)

```bash
# Terminal 1 — backend (tanpa .env = penyimpanan memori)
cd backend && cargo run
# Terminal 2 — dashboard dev server (proxy /api → :3000)
cd frontend && npm install && npm run dev        # buka http://localhost:5173
# Terminal 3 — ESP32 palsu
python tools/simulate_device.py --fast --id DAQ01
```
Kunci operator default saat lokal: `dev-operator`. Untuk mode satu port (seperti produksi):
`cd frontend && npm run build`, lalu `cd backend && cargo run` dan buka http://localhost:3000.

## Halaman dashboard

| Halaman | Fungsi |
|---|---|
| Dashboard | Hero + aroma strip 8 kanal live, status ESP32/Axum/ThingsBoard/Azure, kartu 8 sensor + DHT22, radar fingerprint, kurva respons, hasil terakhir, semua perangkat |
| Analisis & Riwayat | Filter, tabel + detail, tandai QC (bukan hapus), ekspor CSV/Excel/JSON/ZIP dataset, PCA diagnostik (warna Coffee/DAQ/Batch), tabel kontingensi, progres alokasi |
| Klasifikasi Kopi | Form metadata handbook 17.3, Measurement_ID otomatis, mulai/hentikan rekam, progres live, hasil + confidence, perbandingan profil aroma, posisi PCA |
| Perangkat & OTA | Detail ESP32, status sensor, zero baseline, interval sampling, restart, OTA ThingsBoard (unggah .bin, assign, progres fw_state), log aktivitas |
| Tentang | Arsitektur, spesifikasi, info akademis, profil tim |

## Penyesuaian terhadap wireframe (mengikuti handbook)

| Wireframe | Implementasi | Alasan (handbook) |
|---|---|---|
| "Akurasi kemiripan 94,2%" per hasil | **Confidence 0,942** + panel model terpisah (macro-F1 tervalidasi) | 28.2 — accuracy ≠ confidence |
| MQ-2, TGS-822, satuan ppm | 8 sensor sebenarnya, satuan **V** | 8.3, 9.1 — larangan klaim konsentrasi |
| Fase Purging/Sampling/Cleaning | Jendela **Baseline 0–60 / Response 60–400 / Late 400–500 s** | 11.1–11.2 — chamber pasif tanpa pompa |
| Tombol "Hapus" di tabel | **QC_Flag** OK/SUSPECT/REJECT + catatan | 17.5 — data mentah tidak boleh dihapus |
| Ekspektasi varietas/roast | Metadata lengkap: Coffee_ID, Species, Bean_State, Batch, Aliquot, DAQ, massa, dst. | 17.3 |
| BME280, kipas purging | DHT22, tanpa kipas | 8.1, 9.2 |
| PCA thumbnail | PCA diagnostik 3 pewarnaan + tabel kontingensi | 20.2, 15.2 |

## Keamanan

- `.env` **tidak boleh** di-commit (sudah di `.gitignore`). Ganti password Azure SQL lama yang sempat ada di ZIP.
- Pengunjung umum hanya bisa **melihat**. Aksi kontrol butuh `OPERATOR_KEY`; ESP32 login MQTT dengan access token device ThingsBoard (`TB_TOKEN`). `DEVICE_API_KEY` hanya untuk endpoint HTTP lama (simulator).
- Firmware memverifikasi sertifikat TLS (MQTT & HTTPS) dengan CA bundle ESP-IDF.
