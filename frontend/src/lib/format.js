export const fmt = (v, d = 2) =>
  typeof v === 'number' && Number.isFinite(v)
    ? v.toLocaleString('id-ID', { minimumFractionDigits: d, maximumFractionDigits: d })
    : '—'

export const pct = (v, d = 1) => (typeof v === 'number' && Number.isFinite(v) ? `${fmt(v * 100, d)}%` : '—')

export function dateTime(v) {
  if (!v) return '—'
  const d = new Date(v)
  if (Number.isNaN(d.getTime())) return '—'
  return d.toLocaleString('id-ID', { day: '2-digit', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

export function timeOnly(v) {
  if (!v) return '—'
  const d = new Date(v)
  return d.toLocaleTimeString('id-ID', { hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

export function duration(sec) {
  if (sec == null) return '—'
  const s = Math.floor(sec)
  const d = Math.floor(s / 86400), h = Math.floor((s % 86400) / 3600), m = Math.floor((s % 3600) / 60)
  if (d) return `${d} hr ${h} j`
  if (h) return `${h} j ${m} m`
  if (m) return `${m} m ${s % 60} d`
  return `${s % 60} d`
}

export function ago(sec) {
  if (sec == null) return '—'
  if (sec < 5) return 'baru saja'
  if (sec < 60) return `${Math.round(sec)} detik lalu`
  if (sec < 3600) return `${Math.round(sec / 60)} menit lalu`
  return `${Math.round(sec / 3600)} jam lalu`
}

export const bytes = (n) => (n == null ? '—' : n > 1024 * 1024 ? `${fmt(n / 1048576, 2)} MB` : `${fmt(n / 1024, 0)} KB`)

/** Label kelas dari model (mis. "arabica_roasted") → "Arabica Roasted" */
export const prettyClass = (c) =>
  !c ? '—' : c === 'CAPTURE_ONLY' ? 'Belum ada model (capture only)' : c.replace(/[_-]+/g, ' ').replace(/\b\w/g, (m) => m.toUpperCase())
