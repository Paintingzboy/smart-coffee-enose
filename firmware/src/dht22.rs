//! Driver DHT22 (AM2302) — sensor suhu dan kelembapan.
//!
//! Protokol DHT22 menggunakan single-wire dengan timing kritis.
//! GPIO harus dalam mode open-drain dengan pull-up eksternal 4.7kΩ ke 3.3V.
//!
//! Pin yang dipakai: GPIO4 (ubah konstanta DATA_GPIO di main.rs jika perlu)

use anyhow::{bail, Result};
use esp_idf_hal::gpio::{AnyIOPin, InputOutput, PinDriver, Pull};

// Catatan versi: esp-idf-hal 0.47 memakai PinDriver<'d, MODE> (2 parameter generik).

/// Baca waktu dalam mikrodetik sejak boot (resolusi tinggi, tidak terpengaruh scheduler).
#[inline]
fn micros() -> u64 {
    unsafe { esp_idf_svc::sys::esp_timer_get_time() as u64 }
}

/// Tunggu hingga pin mencapai level tertentu, atau timeout.
/// Mengembalikan lama menunggu dalam mikrodetik.
fn wait_level(
    pin: &PinDriver<'_, InputOutput>,
    high: bool,
    timeout_us: u64,
) -> Result<u64> {
    let start = micros();
    loop {
        if pin.is_high() == high {
            return Ok(micros() - start);
        }
        if micros().saturating_sub(start) > timeout_us {
            bail!(
                "DHT22 timeout menunggu {} ({}us)",
                if high { "HIGH" } else { "LOW" },
                timeout_us
            );
        }
    }
}

pub struct Dht22<'d> {
    pin: PinDriver<'d, InputOutput>,
}

impl<'d> Dht22<'d> {
    /// Buat instance DHT22 dari GPIO pin manapun yang mendukung input-output.
    pub fn new(gpio: AnyIOPin<'d>) -> Result<Self> {
        let mut pin = PinDriver::input_output_od(gpio, Pull::Up)?;
        pin.set_high()?;
        // Biarkan bus settle sebelum penggunaan pertama
        std::thread::sleep(std::time::Duration::from_millis(1000));
        Ok(Self { pin })
    }

    /// Baca suhu (°C) dan kelembapan (%).
    /// Mengembalikan (temperature_c, humidity_pct).
    ///
    /// Catatan: DHT22 butuh jeda minimal 2 detik antar pembacaan.
    pub fn read(&mut self) -> Result<(f32, f32)> {
        // === 1. Kirim start signal ===
        // Host tarik LOW minimal 18ms untuk membangunkan DHT22
        self.pin.set_low()?;
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Lepas bus (HIGH), tunggu respons DHT22 (20-40us)
        self.pin.set_high()?;

        // === 2. Baca respons DHT22 ===
        // DHT22 akan tarik LOW (~80us) lalu HIGH (~80us)
        wait_level(&self.pin, false, 60)?; // tunggu LOW response
        wait_level(&self.pin, true, 100)?; // tunggu HIGH response
        wait_level(&self.pin, false, 100)?; // tunggu akhir HIGH response

        // === 3. Baca 40 bit data ===
        let mut data = [0u8; 5];
        for byte in data.iter_mut() {
            let mut value = 0u8;
            for bit in 0..8u8 {
                // Setiap bit: LOW ~50us (preamble), lalu HIGH dengan durasi:
                //   ~26-28us → bit '0'
                //   ~70us    → bit '1'
                wait_level(&self.pin, true, 80)?; // tunggu LOW preamble selesai
                let high_us = wait_level(&self.pin, false, 100)?; // ukur HIGH duration
                if high_us > 40 {
                    // HIGH lebih dari 40us → bit '1'
                    value |= 1 << (7 - bit);
                }
            }
            *byte = value;
        }

        // === 4. Verifikasi checksum ===
        let checksum_expected = data[4];
        let checksum_actual = data[0]
            .wrapping_add(data[1])
            .wrapping_add(data[2])
            .wrapping_add(data[3]);
        if checksum_actual != checksum_expected {
            bail!(
                "DHT22 checksum error: expected {checksum_expected:#04x}, got {checksum_actual:#04x}"
            );
        }

        // === 5. Parse data ===
        // Kelembapan: 16-bit unsigned, satuan 0.1%
        let humidity_raw = u16::from_be_bytes([data[0], data[1]]);
        let humidity = humidity_raw as f32 / 10.0;

        // Suhu: 15-bit unsigned + 1 bit tanda (bit 15 dari data[2])
        let negative = data[2] & 0x80 != 0;
        let temp_raw = u16::from_be_bytes([data[2] & 0x7F, data[3]]);
        // Catatan: bentuk `if neg { -(x / 10.0) } else { x / 10.0 }` memicu bug codegen
        // LLVM Xtensa ("Cannot select PCREL_WRAPPER"). Kalikan 0.1 lalu balik tanda.
        let magnitude = temp_raw as f32 * 0.1;
        let temperature = if negative { -magnitude } else { magnitude };

        // Validasi rentang sensor
        if !(0.0..=100.0).contains(&humidity) {
            bail!("DHT22 kelembapan di luar rentang: {humidity:.1}%");
        }
        if !(-40.0..=80.0).contains(&temperature) {
            bail!("DHT22 suhu di luar rentang: {temperature:.1}°C");
        }

        Ok((temperature, humidity))
    }
}
