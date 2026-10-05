# Deploy ke Railway + Azure SQL + ThingsBoard Cloud

Railway dipilih karena paling sederhana: hubungkan repo GitHub → Railway membaca `Dockerfile`
→ dapat URL HTTPS publik. Satu layanan menyajikan API **dan** dashboard.

## 1. Siapkan repo GitHub
1. Buat repo baru (boleh private), struktur seperti ZIP ini (`backend/`, `frontend/`, `firmware/`, `Dockerfile`, ...).
2. Pastikan tidak ada file `.env` yang ikut ter-commit: `git status` sebelum `git push`.

## 2. Azure SQL
1. Ganti password admin yang lama (Azure Portal → SQL server → *Reset password*).
2. **Networking / Firewall.** Railway tidak memiliki IP keluar tetap pada paket biasa, jadi Azure
   perlu mengizinkan koneksi dari internet:
   - Azure Portal → SQL server → *Networking* → *Public access: Selected networks* →
     tambah aturan `AllowAll` `0.0.0.0` – `255.255.255.255`.
   - Karena itu **password harus kuat** dan hanya disimpan di Railway Variables.
   - Alternatif lebih ketat: Railway Pro *Static Outbound IP*, lalu izinkan IP itu saja.
3. Tidak perlu menjalankan SQL manual — tabel dibuat otomatis saat server start.
   Jika tabel `sensor_readings` versi lama (kolom `mq1..mq8`) masih ada, hapus dulu:
   `DROP TABLE dbo.sensor_readings;` (Query editor di Azure Portal).
4. Database serverless yang *auto-pause* butuh ±1 menit untuk bangun; status "Error" sesaat di
   dashboard setelah lama tidak dipakai itu normal.

## 3. ThingsBoard Cloud (https://eu.thingsboard.cloud)
1. **Entities → Devices → +**: buat device dengan nama **persis** `DAQ01` (dan `DAQ02`… bila ada).
2. Buka device → **Copy access token** → ini `TB_TOKEN` untuk build firmware (per perangkat).
3. Untuk backend: buat API key (menu *API keys* di profil, bila tersedia di akun Anda) → `TB_API_KEY`.
   Bila menu itu tidak ada, pakai `TB_USERNAME` + `TB_PASSWORD` akun tenant.
4. Paket OTA dibuat otomatis oleh dashboard (halaman Perangkat & OTA) — tidak perlu manual.

## 4. Railway
1. https://railway.com → *New Project* → *Deploy from GitHub repo* → pilih repo.
2. Tab **Variables**, isi (lihat `backend/.env.example`):
   ```
   DATABASE_HOST=xxx.database.windows.net
   DATABASE_USER=...
   DATABASE_PASSWORD=...        (password BARU)
   DATABASE_NAME=...
   DEVICE_API_KEY=<acak panjang>
   OPERATOR_KEY=<acak panjang, dibagikan ke anggota tim>
   TB_URL=https://eu.thingsboard.cloud
   TB_API_KEY=...               (atau TB_USERNAME + TB_PASSWORD)
   ```
   Buat kunci acak: `openssl rand -hex 24` atau generator password.
3. Tab **Settings → Networking → Generate Domain** → dapat URL seperti
   `https://smart-coffee-enose-production.up.railway.app`.
4. Build pertama ±5–10 menit (kompilasi Rust). Buka URL → dashboard tampil.
   Status bar harus menunjukkan *Database Azure SQL: Baca/tulis OK* dan *ThingsBoard: Terhubung*.
5. Uji tanpa hardware dari laptop:
   `python tools/simulate_device.py --url https://APP.up.railway.app --key <DEVICE_API_KEY> --id DAQ09 --fast`
   (pakai ID yang tidak dipakai perangkat asli; data simulator sintetis — tandai REJECT setelahnya).

Setiap `git push` ke branch utama otomatis redeploy. ESP32 yang sedang merekam saat redeploy
tetap aman: data ditahan di perangkat dan dikirim ulang setelah server kembali.

## 5. Firmware
Lihat `docs/FIRMWARE.md`. Ringkasnya: build dengan `SERVER_URL=https://APP.up.railway.app`,
`DEVICE_KEY=<DEVICE_API_KEY>`, `DEVICE_ID=DAQ01`, `TB_TOKEN=<token device>`, lalu flash.
