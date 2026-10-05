// Pemrosesan sinyal ringan di browser — hanya untuk visualisasi. Analisis resmi
// dilakukan di notebook (preprocessing di dalam fold, lihat handbook bagian 18 & 23).

const median = (arr) => {
  const a = arr.filter((x) => Number.isFinite(x)).sort((x, y) => x - y)
  if (!a.length) return null
  const m = Math.floor(a.length / 2)
  return a.length % 2 ? a[m] : (a[m - 1] + a[m]) / 2
}

/** x0 per kanal = median pada baseline window (t ≤ 60 s). */
export function baselineOf(samples, until = 60) {
  const win = samples.filter((s) => s.t_s <= until)
  const use = win.length >= 3 ? win : samples.slice(0, Math.min(10, samples.length))
  return Array.from({ length: 8 }, (_, k) => median(use.map((s) => s.v?.[k])))
}

/**
 * Data untuk grafik kurva respons.
 * mode: 'v' (tegangan mentah), 'delta' (ΔV = x − x0), 'frac' (S = (x − x0)/x0)
 */
export function curveData(samples, mode = 'delta') {
  if (!samples?.length) return []
  const x0 = baselineOf(samples)
  return samples.map((s) => {
    const row = { t: s.t_s, temp: s.temp_c, rh: s.rh_pct }
    for (let k = 0; k < 8; k++) {
      const v = s.v?.[k]
      if (!Number.isFinite(v)) { row[`c${k}`] = null; continue }
      if (mode === 'v') row[`c${k}`] = v
      else if (mode === 'delta') row[`c${k}`] = x0[k] == null ? null : v - x0[k]
      else row[`c${k}`] = x0[k] ? (v - x0[k]) / x0[k] : null
    }
    return row
  })
}

/**
 * Fingerprint 8 kanal: respons puncak |ΔV| dinormalisasi oleh norma vektornya sendiri
 * (normalisasi within-measurement → tidak bocor antar sample, handbook 18.3).
 */
export function fingerprintFromSamples(samples) {
  if (!samples?.length) return null
  const x0 = baselineOf(samples)
  const peak = Array.from({ length: 8 }, (_, k) => {
    let m = 0
    for (const s of samples) {
      const v = s.v?.[k]
      if (Number.isFinite(v) && x0[k] != null) m = Math.max(m, Math.abs(v - x0[k]))
    }
    return m
  })
  return normalize(peak)
}

export function normalize(vec) {
  if (!vec) return null
  const v = vec.map((x) => (Number.isFinite(x) ? Math.abs(x) : 0))
  const n = Math.sqrt(v.reduce((a, b) => a + b * b, 0))
  return n > 0 ? v.map((x) => x / n) : v.map(() => 0)
}

export function meanVectors(vectors) {
  const ok = vectors.filter(Boolean)
  if (!ok.length) return null
  return Array.from({ length: ok[0].length }, (_, k) => ok.reduce((a, v) => a + (v[k] || 0), 0) / ok.length)
}

/** Fingerprint dari hasil ringkas (sensor_peak_abs) yang dikirim ESP32. */
export const fingerprintFromResult = (result) =>
  result?.sensor_peak_abs?.length === 8 ? normalize(result.sensor_peak_abs) : null
