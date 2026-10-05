//! Antarmuka model Edge Impulse (EI).
//!
//! Saat ini berupa STUB — mengembalikan None sampai library EI di-integrasikan.
//!
//! ═══════════════════════════════════════════════════════════════
//! CARA INTEGRASI EDGE IMPULSE (setelah training selesai):
//!
//! 1. Di Edge Impulse Studio → Deployment → "C++ Library" → Build & Download
//! 2. Extract zip ke folder `ei_library/` di root proyek ini
//! 3. Di build.rs, aktifkan blok cc::Build (lihat komentar di build.rs)
//! 4. Di Cargo.toml, aktifkan baris cc = "1.0" dan bindgen = "0.69"
//! 5. Ganti implementasi predict() di bawah dengan pemanggilan FFI ke
//!    ei_run_classifier() dari Edge Impulse SDK
//! ═══════════════════════════════════════════════════════════════

/// Hasil prediksi dari model Edge Impulse.
#[derive(Debug, Clone)]
pub struct Prediction {
    /// Label kelas yang diprediksi, misal "arabica_roasted"
    pub label: String,
    /// Skor kepercayaan [0.0..1.0]
    pub confidence: f32,
    /// Waktu inferensi dalam milidetik
    pub inference_time_ms: f32,
    /// Versi model
    pub model_version: String,
}

/// Model Edge Impulse.
pub struct EiModel {
    /// true jika C++ library sudah di-link
    integrated: bool,
}

impl EiModel {
    /// Buat instance model. Secara otomatis mendeteksi apakah EI library tersedia.
    pub fn new() -> Self {
        // Ubah ke `true` setelah library EI berhasil di-link
        let integrated = cfg!(feature = "ei_integrated");
        if integrated {
            log::info!("EI Model: library terhubung, siap inferensi");
        } else {
            log::warn!("EI Model: STUB aktif — jalankan dalam mode capture-only");
            log::warn!("EI Model: Lihat src/ei_model.rs untuk cara integrasi");
        }
        Self { integrated }
    }

    /// Jalankan inferensi pada data sensor mentah.
    ///
    /// # Parameter
    /// - `raw_samples`: slice dari sample, setiap sample = 8 nilai voltase sensor
    /// - Format: [[mq3, mq6, mq7, mq135, tgs2600, tgs2602, tgs2611, tgs2620], ...]
    ///
    /// # Return
    /// - `Some(Prediction)` jika EI library sudah terhubung
    /// - `None` jika masih dalam mode stub (capture only)
    pub fn predict(&self, raw_samples: &[[f32; 8]]) -> Option<Prediction> {
        if !self.integrated {
            return None;
        }
        let _ = raw_samples; // dipakai setelah integrasi Edge Impulse (lihat TODO di bawah)

        // ═══════════════════════════════════════════════════════════
        // TODO: Ganti blok ini dengan pemanggilan EI setelah integrasi
        //
        // Contoh pseudocode untuk FFI ke Edge Impulse:
        //
        // use std::time::Instant;
        // let t0 = Instant::now();
        //
        // // Flatten input: n_samples × 8 → flat array
        // let input: Vec<f32> = raw_samples.iter().flat_map(|s| s.iter().copied()).collect();
        //
        // let mut result = ei_bindings::ei_impulse_result_t::default();
        // let signal = ei_bindings::make_signal(&input, raw_samples.len(), 8);
        // let err = unsafe { ei_bindings::run_classifier(&signal, &mut result, false) };
        //
        // if err != 0 {
        //     log::error!("EI inference error: {err}");
        //     return None;
        // }
        //
        // // Cari label dengan confidence tertinggi
        // let best = (0..result.classification_count)
        //     .map(|i| &result.classification[i])
        //     .max_by(|a, b| a.value.partial_cmp(&b.value).unwrap())?;
        //
        // return Some(Prediction {
        //     label: unsafe { CStr::from_ptr(best.label) }.to_string_lossy().into_owned(),
        //     confidence: best.value,
        //     inference_time_ms: t0.elapsed().as_secs_f32() * 1000.0,
        //     model_version: "ei-v1".to_string(),
        // });
        // ═══════════════════════════════════════════════════════════

        None
    }

    /// Apakah model siap untuk inferensi?
    pub fn is_ready(&self) -> bool {
        self.integrated
    }
}

impl Default for EiModel {
    fn default() -> Self {
        Self::new()
    }
}
