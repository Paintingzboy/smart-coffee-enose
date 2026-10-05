//! Akumulator fitur sederhana untuk 8 kanal sensor MQ.
//!
//! Menghitung:
//! - `sensor_late_mean`: rata-rata voltase pada detik 400-500 (window akhir)
//! - `sensor_peak_abs` : respons puncak absolut terhadap baseline
//!
//! Kedua nilai ini dikirim ke backend Kelas B sebagai bagian dari JSON payload.

use anyhow::Result;

const SENSOR_COUNT: usize = 8;

/// Konstanta pengukuran
const BASELINE_END_S: f32 = 60.0; // 60 detik pertama = baseline
const LATE_START_S: f32 = 400.0; // Window akhir mulai detik ke-400
const ADC_FSR_V: f32 = 4.096; // Full-scale range ADS1115 (PGA 4.096V)

pub struct FeatureAccumulator {
    /// Jumlah sample yang diterima
    pub sample_count: usize,

    /// Akumulasi untuk baseline (detik 0-60)
    baseline_sum: [f64; SENSOR_COUNT],
    baseline_count: usize,

    /// Nilai voltase maksimum yang pernah tercatat
    peak_voltage: [f32; SENSOR_COUNT],

    /// Akumulasi untuk late window (detik 400-500)
    late_sum: [f64; SENSOR_COUNT],
    late_count: usize,

    /// Voltase baseline final (dihitung setelah 60 detik pertama)
    baseline: Option<[f32; SENSOR_COUNT]>,
}

impl FeatureAccumulator {
    pub fn new() -> Self {
        Self {
            sample_count: 0,
            baseline_sum: [0.0; SENSOR_COUNT],
            baseline_count: 0,
            peak_voltage: [0.0; SENSOR_COUNT],
            late_sum: [0.0; SENSOR_COUNT],
            late_count: 0,
            baseline: None,
        }
    }

    /// Tambahkan satu sample. `elapsed_s` adalah waktu sejak mulai pengukuran.
    /// `raw` adalah raw ADC count dari 8 kanal [0..32767].
    pub fn push(&mut self, raw: [u16; 8], elapsed_s: f32) -> Result<()> {
        self.sample_count += 1;

        // Konversi ke voltase
        let voltage: [f32; SENSOR_COUNT] =
            std::array::from_fn(|i| raw[i] as f32 * ADC_FSR_V / 32768.0);

        // Akumulasi baseline (0-60 detik)
        if elapsed_s <= BASELINE_END_S {
            for i in 0..SENSOR_COUNT {
                self.baseline_sum[i] += voltage[i] as f64;
            }
            self.baseline_count += 1;
        } else if self.baseline.is_none() && self.baseline_count > 0 {
            // Hitung rata-rata baseline sekali saat keluar dari window baseline
            let b: [f32; SENSOR_COUNT] =
                std::array::from_fn(|i| (self.baseline_sum[i] / self.baseline_count as f64) as f32);
            self.baseline = Some(b);
        }

        // Track peak voltage
        for i in 0..SENSOR_COUNT {
            if voltage[i] > self.peak_voltage[i] {
                self.peak_voltage[i] = voltage[i];
            }
        }

        // Akumulasi late window (400-500 detik)
        if elapsed_s >= LATE_START_S {
            for i in 0..SENSOR_COUNT {
                self.late_sum[i] += voltage[i] as f64;
            }
            self.late_count += 1;
        }

        Ok(())
    }

    /// Hitung `sensor_late_mean`: rata-rata voltase pada window akhir (400-500s).
    /// Jika window belum cukup terisi, kembalikan rata-rata keseluruhan.
    pub fn late_mean(&self) -> [f32; SENSOR_COUNT] {
        if self.late_count > 0 {
            std::array::from_fn(|i| (self.late_sum[i] / self.late_count as f64) as f32)
        } else if self.sample_count > 0 {
            // Fallback: rata-rata semua sample yang ada
            let total = (self.baseline_sum[0] + self.late_sum[0]) as f64;
            let _ = total; // suppress warning
            std::array::from_fn(|i| {
                ((self.baseline_sum[i] + self.late_sum[i])
                    / (self.baseline_count + self.late_count).max(1) as f64) as f32
            })
        } else {
            [0.0; SENSOR_COUNT]
        }
    }

    /// Hitung `sensor_peak_abs`: respons puncak absolut terhadap baseline.
    /// Jika baseline belum tersedia, gunakan peak raw.
    pub fn peak_abs(&self) -> [f32; SENSOR_COUNT] {
        match self.baseline {
            Some(baseline) => {
                std::array::from_fn(|i| (self.peak_voltage[i] - baseline[i]).abs())
            }
            None => self.peak_voltage,
        }
    }

    /// Rata-rata suhu dari semua sample yang diterima.
    /// Hitung terpisah di main.rs menggunakan sum/count.
    #[allow(dead_code)]
    pub fn is_enough_data(&self) -> bool {
        self.sample_count >= 100 // Minimal 100 sample untuk hasil bermakna
    }
}

impl Default for FeatureAccumulator {
    fn default() -> Self {
        Self::new()
    }
}
