import { useMemo } from 'react'
import { Radio, Cpu, PlayCircle } from 'lucide-react'
import { useApp } from '../lib/app'
import { useDeviceLive, usePolling } from '../lib/hooks'
import { SITE } from '../config/site'
import { StatusBar, SensorCards, AromaStrip, referenceVector } from '../components/Live'
import { ResponseCurve, FingerprintRadar } from '../components/Charts'
import { PredictionCard, ModelPanel } from '../components/ResultCard'
import { Panel, Badge, Empty, Progress } from '../components/ui'
import { DEVICE_STATES } from '../lib/sensors'
import { fingerprintFromSamples, normalize } from '../lib/signal'
import { ago, dateTime } from '../lib/format'

function liveFingerprint(device, history, samples) {
  if (samples?.length > 5) return { vec: fingerprintFromSamples(samples), from: 'pengukuran terakhir' }
  const { ref } = referenceVector(device, history)
  const v = device?.live?.v
  if (!ref || !v) return { vec: null }
  const d = v.map((x, k) => (Number.isFinite(x) && Number.isFinite(ref[k]) ? Math.abs(x - ref[k]) : 0))
  return { vec: d.some((x) => x > 1e-4) ? normalize(d) : null, from: 'pembacaan live' }
}

export default function DashboardPage() {
  const { status, statusError, device, deviceId, devices, model } = useApp()
  const live = useDeviceLive(deviceId, 2000)
  const stats = usePolling('/stats', 10000)
  const dev = live.device || device
  const fp = useMemo(() => liveFingerprint(dev, live.history, live.samples), [dev, live.history, live.samples])
  const latest = stats.data?.latest
  const recording = dev?.state === 'recording'

  return (
    <>
      <section className="hero-bg text-white">
        <div className="mx-auto grid max-w-7xl gap-8 px-4 py-10 sm:px-6 lg:grid-cols-[1.15fr_1fr] lg:items-center lg:py-14">
          <div className="min-w-0">
            <p className="inline-block max-w-full rounded-md bg-white/10 px-2.5 py-1 text-xs text-white/80">
              {SITE.department} · {SITE.campus}
            </p>
            <h1 className="mt-4 font-display text-[2.6rem] font-bold leading-[1.05] sm:text-5xl lg:text-6xl">
              SMART COFFEE<br />E-NOSE DASHBOARD
            </h1>
            <p className="mt-4 max-w-xl text-base text-white/80 sm:text-lg">
              Platform monitoring real-time dan analisis karakteristik aroma biji kopi berbasis Electronic Nose — 8 sensor gas MOS, ESP32-S3, dan TinyML.
            </p>
            <div className="mt-6 flex flex-wrap gap-3">
              <a href="#/classification" className="btn bg-roast px-5 py-2.5 text-white hover:bg-roast-dark"><PlayCircle className="h-4 w-4" />Mulai pengujian sampel</a>
              <a href="#/analytics" className="btn border border-white/25 px-5 py-2.5 text-white hover:bg-white/10">Lihat riwayat data</a>
            </div>
          </div>
          <AromaStrip device={dev} samples={live.samples} />
        </div>
      </section>

      <div className="mx-auto max-w-7xl space-y-6 px-4 py-6 sm:px-6">
        <StatusBar status={status} device={dev} error={statusError} />

        {recording && (
          <a href="#/classification" className="panel flex flex-wrap items-center gap-4 p-4 hover:border-signal">
            <Radio className="h-5 w-5 text-signal pulse-dot" />
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium">Sedang merekam {dev.measurement_id}</p>
              <div className="mt-2"><Progress value={(dev.samples_done || 0) / (dev.samples_total || 1)} tone="signal" label="Progres rekaman" /></div>
            </div>
            <span className="num text-sm text-ink-soft">{dev.samples_done}/{dev.samples_total} sampel</span>
          </a>
        )}

        <div className="grid grid-cols-1 gap-6 xl:grid-cols-[1.6fr_1fr]">
          <Panel title="Live sensor monitoring" subtitle={dev ? `${dev.device_id} · diperbarui ${ago(dev.seconds_since_seen)}` : 'Belum ada perangkat'}
            actions={dev && !dev.online && <Badge tone="danger">Data basi — perangkat offline</Badge>}>
            {dev ? <SensorCards device={dev} history={live.history} samples={live.samples} /> : (
              <Empty icon={Cpu} title="Belum ada ESP32 yang terhubung">
                Nyalakan ESP32-S3 dengan firmware terbaru. Perangkat akan muncul otomatis di sini dalam beberapa detik setelah tersambung WiFi.
              </Empty>
            )}
          </Panel>
          <Panel title="Aroma fingerprint" subtitle={fp.from ? `Respons |ΔV| 8 kanal, dinormalisasi — ${fp.from}` : 'Respons relatif 8 kanal'}>
            <FingerprintRadar a={fp.vec} aLabel="Fingerprint" />
            <p className="mt-1 text-xs text-ink-faint">Bentuk pola yang dibandingkan, bukan besarnya. Sensor MOS tidak selektif — pola gabunganlah yang membedakan aroma.</p>
          </Panel>
        </div>

        <Panel title="Kurva respons sensor" subtitle={live.measurementId ? `Pengukuran ${live.measurementId}` : 'Pengukuran aktif atau terakhir pada perangkat ini'}>
          <ResponseCurve samples={live.samples} emptyText="Tekan “Mulai rekam” di halaman Klasifikasi Kopi untuk memulai pengukuran 500 detik." />
        </Panel>

        <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
          <Panel title="Hasil klasifikasi terakhir" subtitle={latest ? dateTime(latest.finished_at || latest.created_at) : 'Belum ada pengukuran selesai'}>
            {latest ? <PredictionCard row={latest} threshold={model?.confidence_threshold} /> : <Empty title="Belum ada hasil">Hasil pertama tampil setelah satu pengukuran selesai.</Empty>}
            <div className="mt-4"><ModelPanel model={model} /></div>
          </Panel>
          <Panel title="Semua perangkat" subtitle="Lima unit DAQ dapat dipantau sekaligus">
            {devices.length ? (
              <ul className="divide-y divide-line">
                {devices.map((d) => {
                  const st = DEVICE_STATES[d.state] || { label: d.state, tone: 'neutral' }
                  return (
                    <li key={d.device_id} className="flex flex-wrap items-center gap-3 py-3">
                      <Cpu className="h-5 w-5 text-ink-soft" aria-hidden />
                      <div className="min-w-0 flex-1">
                        <p className="font-medium">{d.device_id} <span className="text-xs font-normal text-ink-faint">fw {d.fw_version}</span></p>
                        <p className="text-xs text-ink-soft">{d.measurement_id || 'Tidak ada pengukuran aktif'} · {ago(d.seconds_since_seen)}</p>
                      </div>
                      <Badge tone={d.online ? st.tone : 'danger'} dot pulse={d.online && st.tone === 'live'}>{d.online ? st.label : 'Offline'}</Badge>
                    </li>
                  )
                })}
              </ul>
            ) : <Empty icon={Cpu} title="Belum ada perangkat">ESP32 muncul otomatis saat mengirim heartbeat pertama.</Empty>}
            {stats.data && (
              <div className="mt-4 grid grid-cols-3 gap-3 border-t border-line pt-4 text-center">
                <div><p className="num text-2xl font-semibold">{stats.data.by_status?.completed || 0}</p><p className="text-xs text-ink-soft">Pengukuran selesai</p></div>
                <div><p className="num text-2xl font-semibold">{stats.data.coffees?.length || 0}<span className="text-base text-ink-faint">/18</span></p><p className="text-xs text-ink-soft">Coffee_ID terukur</p></div>
                <div><p className="num text-2xl font-semibold">{(stats.data.by_qc?.SUSPECT || 0) + (stats.data.by_qc?.REJECT || 0)}</p><p className="text-xs text-ink-soft">Ditandai QC</p></div>
              </div>
            )}
          </Panel>
        </div>
      </div>
    </>
  )
}
