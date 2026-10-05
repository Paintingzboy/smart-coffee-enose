-- Referensi skema. TIDAK perlu dijalankan manual: backend membuat tabel otomatis
-- saat start (IF OBJECT_ID ... IS NULL). Simpan file ini untuk dokumentasi laporan.
--
-- PENTING: bila tabel sensor_readings versi LAMA (kolom mq1..mq8) masih ada,
-- hapus dulu:  DROP TABLE dbo.sensor_readings;

-- 1. Metadata pengukuran (handbook 17.3) — satu baris per Measurement_ID, termasuk yang gagal
CREATE TABLE dbo.measurement_sessions (
    measurement_id NVARCHAR(64) NOT NULL PRIMARY KEY,
    device_id NVARCHAR(50) NOT NULL,
    daq_id NVARCHAR(20) NULL, batch_id NVARCHAR(20) NULL, group_id NVARCHAR(20) NULL,
    coffee_id NVARCHAR(20) NULL, aliquot_id NVARCHAR(20) NULL, species NVARCHAR(20) NULL,
    bean_state NVARCHAR(20) NULL, roast_level NVARCHAR(20) NULL, mass_g FLOAT NULL,
    operator_name NVARCHAR(50) NULL, protocol_ver NVARCHAR(20) NULL, run_order INT NULL,
    heating_s INT NULL, duration_s INT NOT NULL, sample_period_ms INT NOT NULL,
    warmup_min INT NULL, room_t FLOAT NULL, room_rh FLOAT NULL, divider_ratio FLOAT NULL,
    notes NVARCHAR(1000) NULL,
    qc_flag NVARCHAR(10) NOT NULL DEFAULT 'OK',          -- OK / SUSPECT / REJECT
    status NVARCHAR(20) NOT NULL,                        -- queued/recording/completed/failed/cancelled/interrupted
    dry_run BIT NOT NULL DEFAULT 0, error NVARCHAR(500) NULL,
    samples_received INT NOT NULL DEFAULT 0,
    created_at DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME(), started_at DATETIME2 NULL, finished_at DATETIME2 NULL
);

-- 2. Data mentah 1 baris per detik (handbook 17.4) — tidak pernah dihapus/diubah
CREATE TABLE dbo.measurement_samples (
    measurement_id NVARCHAR(64) NOT NULL, idx INT NOT NULL, t_s FLOAT NOT NULL, ts NVARCHAR(40) NULL,
    raw1 INT NULL, raw2 INT NULL, raw3 INT NULL, raw4 INT NULL, raw5 INT NULL, raw6 INT NULL, raw7 INT NULL, raw8 INT NULL,
    v1 FLOAT NULL, v2 FLOAT NULL, v3 FLOAT NULL, v4 FLOAT NULL, v5 FLOAT NULL, v6 FLOAT NULL, v7 FLOAT NULL, v8 FLOAT NULL,
    temp_c FLOAT NULL, rh_pct FLOAT NULL, dht_age_s INT NULL,
    CONSTRAINT PK_measurement_samples PRIMARY KEY CLUSTERED (measurement_id, idx) WITH (IGNORE_DUP_KEY = ON)
);

-- 3. Hasil akhir dari ESP32 (fitur ringkas + inferensi TinyML) — format sama seperti sebelumnya
CREATE TABLE dbo.sensor_readings (
    id INT IDENTITY(1,1) PRIMARY KEY,
    device_id NVARCHAR(50) NOT NULL, fw_version NVARCHAR(20) NULL, measurement_id NVARCHAR(64) NOT NULL,
    daq_id NVARCHAR(20) NULL, batch_id NVARCHAR(20) NULL, group_id NVARCHAR(20) NULL, coffee_id NVARCHAR(20) NULL,
    aliquot_id NVARCHAR(20) NULL, species NVARCHAR(20) NULL, bean_state NVARCHAR(20) NULL,
    predicted_class NVARCHAR(50) NULL, confidence FLOAT NULL, model_version NVARCHAR(50) NULL,
    temperature FLOAT NULL, humidity FLOAT NULL, sample_count INT NULL, sample_duration_s FLOAT NULL,
    inference_time_ms FLOAT NULL, sensor_late_mean NVARCHAR(MAX) NULL, sensor_peak_abs NVARCHAR(MAX) NULL,
    seq BIGINT NULL, reading_timestamp NVARCHAR(40) NULL, created_at DATETIME2 DEFAULT SYSUTCDATETIME()
);
CREATE INDEX IX_sensor_readings_measurement_id ON dbo.sensor_readings (measurement_id);
