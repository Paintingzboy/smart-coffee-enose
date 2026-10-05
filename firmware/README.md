# Firmware Smart Coffee E-Nose (ESP32-S3, Rust)

Lihat `../docs/FIRMWARE.md` untuk build, flash, OTA, dan Edge Impulse.

| File | Isi |
|---|---|
| `src/main.rs` | Boot, WiFi, NTP, membuat thread `acq` & `net`, perintah serial cadangan |
| `src/config.rs` | Konfigurasi dari environment variable saat build |
| `src/acquisition.rs` | Warm-up, preview live, rekam N sampel dengan timer monotonik, fitur, TinyML |
| `src/net.rs` | Klien HTTPS, heartbeat + perintah, unggah data mentah & hasil (dengan antrian retry) |
| `src/ota.rs` | OTA ThingsBoard (HTTP Device API), SHA-256, fw_state, rollback |
| `src/ads1115.rs` | 2× ADS1115, dibaca per chip |
| `src/dht22.rs` | DHT22 bit-bang |
| `src/features.rs` | Late mean & peak |abs| per kanal |
| `src/ei_model.rs` | Antarmuka Edge Impulse (stub sampai library diekspor) |
| `src/shared.rs` | State & pesan antar-thread |
