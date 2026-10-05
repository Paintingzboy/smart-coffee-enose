import { Cpu, Server, Cloud, Database, Thermometer, Droplets, Wifi } from 'lucide-react'
import { SENSORS, ADC_FSR_V, NEAR_SATURATION_V, DEVICE_STATES } from '../lib/sensors'
import { fmt, ago } from '../lib/format'
import { Badge, Sparkline } from './ui'

function StatusCell({ icon: Icon, name, tone, value, detail }) {
  return (
    <div className="flex min-w-0 items-start gap-3 px-4 py-3">
      <Icon className="mt-0.5 h-5 w-5 shrink-0 text-ink-soft" aria-hidden />
      <div className="min-w-0">
        <p className="text-xs text-ink-soft">{name}</p>
        <div className="mt-0.5 flex flex-wrap items-center gap-2">
          <Badge tone={tone} dot pulse={tone === 'live'}>{value}</Badge>
        </div>
        {detail && <p className="mt-1 truncate text-xs text-ink-faint">{detail}</p>}
      </div>
    </div>
  )
}

export function StatusBar({ status, device, error }) {
  const db = status?.database, tb = status?.thingsboard, be = status?.backend
  const st = DEVICE_STATES[device?.state] || { label: device?.state || '—', tone: 'neutral' }
  return (
    <div className="panel grid divide-y divide-line sm:grid-cols-2 sm:divide-y-0 lg:grid-cols-4 lg:divide-x [&>*:nth-child(2)]:sm:border-l [&>*:nth-child(3)]:sm:border-t [&>*:nth-child(4)]:sm:border-l [&>*:nth-child(4)]:sm:border-t lg:[&>*]:!border-t-0 lg:[&>*:nth-child(3)]:!border-l lg:[&>*:nth-child(2)]:!border-l">
      <StatusCell
        icon={Cpu}
        name={`ESP32-S3 ${device?.device_id || ''}`}
        tone={!device ? 'neutral' : device.online ? (st.tone === 'danger' ? 'danger' : 'ok') : 'danger'}
        value={!device ? 'Belum terhubung' : device.online ? `Online · ${st.label}` : 'Offline'}
        detail={device ? `RSSI ${device.rssi ?? '—'} dBm · terakhir ${ago(device.seconds_since_seen)}` : 'Nyalakan ESP32 — muncul otomatis di sini'}
      />
      <StatusCell
        icon={Server}
        name="Backend Rust (Axum)"
        tone={error ? 'danger' : be ? 'ok' : 'neutral'}
        value={error ? 'Tidak terhubung' : be ? 'Terhubung' : 'Memeriksa…'}
        detail={be ? `v${be.version} · respons ${fmt(be.latency_ms, 1)} ms` : error}
      />
      <StatusCell
        icon={Cloud}
        name="ThingsBoard (OTA)"
        tone={!tb ? 'neutral' : !tb.configured ? 'warn' : tb.ok ? 'ok' : 'danger'}
        value={!tb ? '—' : !tb.configured ? 'Belum dikonfigurasi' : tb.ok ? 'Terhubung' : 'Gagal login'}
        detail={tb?.url?.replace('https://', '') || tb?.error}
      />
      <StatusCell
        icon={Database}
        name={db?.kind === 'memory' ? 'Database (memori sementara)' : 'Database Azure SQL'}
        tone={!db ? 'neutral' : db.ok ? (db.kind === 'memory' ? 'warn' : 'ok') : 'danger'}
        value={!db ? '—' : db.ok ? 'Baca/tulis OK' : 'Error'}
        detail={db?.ok ? `${db.readings ?? 0} hasil · ${(db.samples ?? 0).toLocaleString('id-ID')} baris mentah` : db?.error}
      />
    </div>
  )
}

/** Nilai referensi untuk ΔV kartu sensor: zero baseline perangkat, atau titik tertua riwayat live. */
export function referenceVector(device, history) {
  if (device?.zero_baseline?.length === 8) return { ref: device.zero_baseline, label: 'zero baseline' }
  const first = history?.find((h) => h.v?.some((x) => Number.isFinite(x)))
  return { ref: first?.v || null, label: 'awal riwayat' }
}

export function SensorCards({ device, history, samples }) {
  const recording = device?.state === 'recording' && samples?.length
  const current = recording ? samples[samples.length - 1].v : device?.live?.v
  const series = (k) => (recording ? samples.slice(-120).map((s) => s.v?.[k]) : (history || []).map((h) => h.v?.[k]))
  const { ref, label } = referenceVector(device, history)
  const temp = recording ? samples[samples.length - 1].temp_c : device?.live?.temp_c
  const rh = recording ? samples[samples.length - 1].rh_pct : device?.live?.rh_pct

  return (
    <div>
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        {SENSORS.map((s, k) => {
          const v = current?.[k]
          const ok = Number.isFinite(v)
          const d = ok && Number.isFinite(ref?.[k]) ? v - ref[k] : null
          const tone = !device?.online ? 'neutral' : !ok ? 'danger' : v >= NEAR_SATURATION_V ? 'warn' : 'ok'
          const text = !device?.online ? 'Offline' : !ok ? 'Tidak terbaca' : v >= NEAR_SATURATION_V ? 'Dekat batas ADC' : 'Normal'
          return (
            <article key={s.key} className="rounded-xl border border-line bg-white p-3">
              <div className="flex items-start justify-between gap-1">
                <div className="min-w-0">
                  <p className="text-xs text-ink-faint">{s.code}</p>
                  <h3 className="whitespace-nowrap font-display text-base font-semibold" style={{ color: s.color }}>{s.name}</h3>
                </div>
                <Badge tone={tone} className="shrink-0 !px-1.5 text-[11px]">{text}</Badge>
              </div>
              <p className="num mt-2 text-2xl font-semibold text-ink">
                {ok ? fmt(v, 3) : '—'}<span className="ml-1 text-sm font-normal text-ink-soft">V</span>
              </p>
              <p className="num text-xs text-ink-soft">
                ΔV {d == null ? '—' : `${d >= 0 ? '+' : ''}${fmt(d * 1000, 1)} mV`}
              </p>
              <div className="mt-2"><Sparkline values={series(k)} color={s.color} /></div>
            </article>
          )
        })}
      </div>
      <div className="mt-3 grid gap-3 sm:grid-cols-2">
        <div className="flex items-center gap-3 rounded-xl border border-line bg-white p-3">
          <Thermometer className="h-6 w-6 text-roast" aria-hidden />
          <div>
            <p className="text-xs text-ink-soft">DHT22 · Temperatur</p>
            <p className="num text-xl font-semibold">{fmt(temp, 1)} <span className="text-sm font-normal text-ink-soft">°C</span></p>
          </div>
        </div>
        <div className="flex items-center gap-3 rounded-xl border border-line bg-white p-3">
          <Droplets className="h-6 w-6 text-signal" aria-hidden />
          <div>
            <p className="text-xs text-ink-soft">DHT22 · Kelembapan relatif</p>
            <p className="num text-xl font-semibold">{fmt(rh, 1)} <span className="text-sm font-normal text-ink-soft">%RH</span></p>
          </div>
        </div>
      </div>
      <p className="mt-2 text-xs text-ink-faint">
        Nilai adalah tegangan di pin ADS1115 (FSR ±{ADC_FSR_V} V), bukan konsentrasi gas. ΔV relatif terhadap {label}.
      </p>
    </div>
  )
}

/** Strip 8 kanal di hero: tinggi batang = tegangan sensor terhadap FSR ADC. */
export function AromaStrip({ device, samples }) {
  const recording = device?.state === 'recording' && samples?.length
  const v = recording ? samples[samples.length - 1].v : device?.live?.v
  // Skala dinamis (min 2 V) agar perubahan kecil tetap terlihat; angka di atas batang = nilai asli.
  const top = Math.max(2, ...(v || []).filter(Number.isFinite).map((x) => x * 1.15))
  return (
    <div className="rounded-panel border border-white/15 bg-black/25 p-4 backdrop-blur-sm">
      <div className="mb-3 flex items-center justify-between gap-2 text-xs text-white/70">
        <span className="flex items-center gap-1.5">
          <Wifi className="h-3.5 w-3.5" aria-hidden />
          {device ? `${device.device_id} · ${device.online ? (DEVICE_STATES[device.state]?.label || device.state) : 'offline'}` : 'Menunggu perangkat'}
        </span>
        <span className="num">8 kanal · skala 0–{fmt(top, 1)} V</span>
      </div>
      <div className="flex h-36 items-end gap-1 sm:gap-3" role="img" aria-label="Level tegangan delapan sensor gas">
        {SENSORS.map((s, k) => {
          const x = v?.[k]
          const h = Number.isFinite(x) ? Math.max(0.03, Math.min(1, x / top)) : 0.03
          return (
            <div key={s.key} className="flex h-full min-w-0 flex-1 flex-col items-center justify-end gap-1.5">
              <span className="num text-[10px] text-white/70">{Number.isFinite(x) ? fmt(x, 2) : '—'}</span>
              <div className="w-full rounded-t-md transition-[height] duration-700" style={{ height: `${h * 100}%`, background: Number.isFinite(x) ? s.color : 'rgba(255,255,255,.15)' }} />
              <span className="w-full truncate text-center text-[9px] text-white/80 sm:text-[10px]" title={s.name}>{s.name.replace('TGS', '')}</span>
            </div>
          )
        })}
      </div>
    </div>
  )
}
