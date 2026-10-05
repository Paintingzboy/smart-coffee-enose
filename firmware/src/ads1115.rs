//! Driver ADS1115 — ADC 16-bit 4 kanal via I2C.
//!   Chip A (0x48, ADDR=GND): MQ-3, MQ-6, MQ-7, MQ-135        → kanal 0..3
//!   Chip B (0x49, ADDR=VDD): TGS2600, TGS2602, TGS2611, TGS2620 → kanal 4..7
//!
//! Setiap chip dibaca terpisah: bila satu chip tidak terpasang/terputus, chip lain
//! tetap terbaca dan firmware TIDAK berhenti (kanal yang hilang bernilai None).

use anyhow::Result;
use esp_idf_hal::i2c::I2cDriver;
use std::{thread, time::Duration};

pub const ADDR_A: u8 = 0x48;
pub const ADDR_B: u8 = 0x49;

const REG_CONVERSION: u8 = 0x00;
const REG_CONFIG: u8 = 0x01;

const OS_START_SINGLE: u16 = 0x8000;
/// PGA ±4,096 V → 125 µV/LSB. Pastikan tegangan di pin ADC ≤ VDD (lihat handbook 10.3).
const PGA_4096V: u16 = 0x0200;
pub const FSR_V: f32 = 4.096;
const MODE_SINGLE: u16 = 0x0100;
/// 128 SPS: lebih tahan derau daripada 860 SPS, tetap jauh di bawah 1 detik untuk 8 kanal.
const DR_128SPS: u16 = 0x0080;
const COMP_DISABLE: u16 = 0x0003;
const MUX: [u16; 4] = [0x4000, 0x5000, 0x6000, 0x7000];
const I2C_TIMEOUT_TICKS: u32 = 50;

fn read_channel(i2c: &mut I2cDriver<'_>, addr: u8, ch: usize) -> Result<u16> {
    let cfg = OS_START_SINGLE | MUX[ch] | PGA_4096V | MODE_SINGLE | DR_128SPS | COMP_DISABLE;
    i2c.write(addr, &[REG_CONFIG, (cfg >> 8) as u8, cfg as u8], I2C_TIMEOUT_TICKS)?;
    // Konversi 128 SPS ≈ 7,8 ms; poll bit OS sampai selesai (maks ±40 ms)
    thread::sleep(Duration::from_millis(8));
    for _ in 0..16 {
        let mut buf = [0u8; 2];
        i2c.write_read(addr, &[REG_CONFIG], &mut buf, I2C_TIMEOUT_TICKS)?;
        if u16::from_be_bytes(buf) & 0x8000 != 0 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    let mut buf = [0u8; 2];
    i2c.write_read(addr, &[REG_CONVERSION], &mut buf, I2C_TIMEOUT_TICKS)?;
    // Keluaran signed; mode single-ended hanya rentang positif.
    Ok(i16::from_be_bytes(buf).max(0) as u16)
}

/// Baca 4 kanal satu chip.
pub fn read_chip(i2c: &mut I2cDriver<'_>, addr: u8) -> Result<[u16; 4]> {
    let mut out = [0u16; 4];
    for (ch, slot) in out.iter_mut().enumerate() {
        *slot = read_channel(i2c, addr, ch)?;
    }
    Ok(out)
}

/// Apakah chip menjawab di bus I2C?
pub fn probe(i2c: &mut I2cDriver<'_>, addr: u8) -> bool {
    let mut buf = [0u8; 2];
    i2c.write_read(addr, &[REG_CONFIG], &mut buf, I2C_TIMEOUT_TICKS).is_ok()
}

/// Hasil pembacaan 8 kanal. `None` = chip tidak terbaca.
pub struct Reading {
    pub raw: [Option<u16>; 8],
    pub chip_a_ok: bool,
    pub chip_b_ok: bool,
}

/// Urutan keluaran: [MQ3, MQ6, MQ7, MQ135, TGS2600, TGS2602, TGS2611, TGS2620]
pub fn read_all(i2c: &mut I2cDriver<'_>) -> Reading {
    let mut raw = [None; 8];
    let a = read_chip(i2c, ADDR_A).ok();
    let b = read_chip(i2c, ADDR_B).ok();
    if let Some(a) = a {
        for k in 0..4 {
            raw[k] = Some(a[k]);
        }
    }
    if let Some(b) = b {
        for k in 0..4 {
            raw[k + 4] = Some(b[k]);
        }
    }
    Reading { raw, chip_a_ok: a.is_some(), chip_b_ok: b.is_some() }
}

pub fn to_voltage(raw: u16) -> f32 {
    raw as f32 * FSR_V / 32768.0
}
