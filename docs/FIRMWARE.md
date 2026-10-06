# Firmware ESP32-S3 (Rust) — build, flash, OTA, Edge Impulse

**Status verifikasi:** firmware ini sudah dikompilasi & di-link penuh (`cargo build --release`, 0 error,
0 warning) dengan ESP-IDF v5.5.3 dan esp-idf-svc/hal sesuai `Cargo.lock`. Ukuran aplikasi ±1,27 MB
dari partisi OTA 1,94 MB. Yang belum bisa diuji tanpa hardware: perilaku di board fisik (WiFi,
I2C, DHT22, OTA nyata) — lakukan uji T-01…T-20 handbook setelah flash.

## Perubahan dari versi sebelumnya

| Sebelumnya | Sekarang |
|---|---|
| Menunggu `START|...` lewat kabel serial | Menerima perintah dashboard sebagai **RPC MQTT ThingsBoard** (serial tetap ada sebagai cadangan offline) |
| Kirim hasil ke IP LAN `192.168.x.x:3000` (HTTP mentah) | **MQTT/TLS** ke ThingsBoard (`mqtt.eu.thingsboard.cloud:8883`), sertifikat diverifikasi; backend Railway membaca dari ThingsBoard |
| ADS1115 tidak terpasang → `?` menghentikan firmware | Chip dibaca terpisah; perangkat tetap online dan melaporkan sensor mana yang hilang |
| DHT22 gagal → menunggu selamanya | Non-blocking, nilai terakhir di-hold (zero-order hold), `dht_age_s` dicatat |
| Data mentah hanya di log serial | Dikirim ke database tiap ±5 s (+ tetap dicetak `RAW_CSV` ke serial) |
| Akuisisi & jaringan satu thread | Thread **acq** dan **net** terpisah (handbook 10.5) — WiFi lambat tidak menimbulkan jitter |
| `check_ota()` kosong | OTA ThingsBoard lengkap: SHA-256, laporan `fw_state`, rollback otomatis |
| `PinDriver<'_, AnyIOPin, InputOutput>` (tidak terkompilasi) | `PinDriver<'_, InputOutput>` — sesuai esp-idf-hal 0.47 yang terkunci di Cargo.lock |
| Suhu negatif DHT22: `if neg { -(x/10.0) } else { x/10.0 }` | `x * 0.1` lalu balik tanda — pola lama memicu bug codegen LLVM Xtensa ("Cannot select PCREL_WRAPPER") |
| `sdkconfig.defaults` diawali BOM UTF-8 | BOM dihapus |
| Partisi di `sdkconfig.defaults` (path relatif → build gagal) | Partisi lewat `espflash.toml` |
| `ESP_TLS_INSECURE` + skip verifikasi | Dihapus — memakai CA bundle ESP-IDF |
| ADS1115 860 SPS | 128 SPS (lebih rendah derau; 8 kanal tetap < 100 ms) |

## Build

Prasyarat (sekali saja): `espup install` (toolchain Rust Xtensa), `cargo install ldproxy espflash`.
Build pertama mengunduh ESP-IDF v5.5.3 ke `.embuild/` (±4 GB) dan memakan waktu lama; berikutnya cepat.

Nilai rahasia diisi lewat environment variable saat build (tidak ditulis di kode/Git).

PowerShell (Windows):
```powershell
$env:WIFI_SSID="NamaWiFiLab"; $env:WIFI_PASS="passwordwifi"
$env:DEVICE_ID="DAQ01"                 # sama dengan nama device di ThingsBoard
$env:TB_TOKEN="<access token device DAQ01 di ThingsBoard>"   # WAJIB: username MQTT
$env:WARMUP_S="60"                     # minimal sebelum boleh rekam (SOP: tunggu ≥30 menit)
cargo build --release
```
Linux/macOS: ganti `$env:X="..."` dengan `export X=...`.

> Variabel ini dibaca saat **kompilasi**. Setelah mengubahnya, jalankan `cargo clean -p teknologi-io-t-a`
> sebelum build agar nilainya ikut berubah.

## Flash pertama (kabel USB)

```bash
espflash flash --monitor target/xtensa-esp32s3-espidf/release/teknologi-io-t-a
```
`espflash.toml` otomatis memakai `partitions.csv` (factory + ota_0 + ota_1) dan flash 16 MB.

**Rollback OTA** butuh bootloader buatan ESP-IDF (bukan bootloader bawaan espflash):
```bash
BL=$(find target/xtensa-esp32s3-espidf/release/build -name bootloader.bin | head -1)
espflash flash --bootloader "$BL" --monitor target/xtensa-esp32s3-espidf/release/teknologi-io-t-a
```

Setelah menyala, log serial menampilkan `WiFi tersambung` lalu `MQTT tersambung`. Di dashboard, perangkat
muncul **online** (status *Warm-up* dulu, lalu *Siap merekam*).

## Update firmware lewat OTA (tanpa kabel)

1. Naikkan `version` di `Cargo.toml` (mis. `1.0.0` → `1.0.1`).
2. Build seperti di atas, lalu buat image aplikasi:
   ```bash
   espflash save-image --chip esp32s3 target/xtensa-esp32s3-espidf/release/teknologi-io-t-a firmware-1.0.1.bin
   ```
3. Dashboard → **Perangkat & OTA** → pilih file `.bin`, isi versi `1.0.1` → **Unggah & push update**.
4. Progres tampil (DOWNLOADING → VERIFIED → UPDATING → UPDATED). Perangkat restart sendiri.
   OTA tidak pernah dimulai saat perangkat merekam.
5. Bila firmware baru gagal terhubung ke server lalu restart, bootloader kembali ke versi lama
   (status FAILED "Rollback" dilaporkan ke ThingsBoard).

## Edge Impulse — urutan kerja

Model belum dilatih, jadi firmware berjalan **capture-only** (`predicted_class = CAPTURE_ONLY`).
Itu urutan yang benar: kumpulkan dataset dulu.

1. Rekam pengukuran lewat dashboard (alokasi DAQ × batch handbook 16.4).
2. Dashboard → Analisis → **Dataset ZIP** → berisi `01_raw_data/*.csv` + metadata.
3. Di Edge Impulse, unggah CSV dan **bagi train/test berdasarkan Coffee_ID** (bukan acak) —
   handbook 25.4. Simpan bukti bahwa tidak ada Coffee_ID di dua sisi.
4. Latih, validasi juga di notebook sendiri (GroupKFold, LODO, LOBO).
5. Deployment → **C++ library** → ekstrak ke `firmware/ei_library/`, aktifkan blok `cc`/`bindgen`
   di `build.rs` dan `Cargo.toml`, lalu ganti isi `predict()` di `src/ei_model.rs` (Opsi A/B handbook 26.2).
   Fitur di perangkat harus identik urutan & satuannya dengan saat pelatihan — jalankan *parity test*.
6. Isi `MODEL_NAME`, `MODEL_VERSION`, `MODEL_METRIC` di Railway agar panel model di dashboard benar.

Tidak ada API key Edge Impulse yang perlu diberikan ke firmware maupun backend.
