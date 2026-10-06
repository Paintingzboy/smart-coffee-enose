import { useEffect, useMemo, useState } from 'react'
import { Play, Square, FlaskConical, CheckCircle2, Circle, Loader2, Download } from 'lucide-react'
import { useApp } from '../lib/app'
import { api, fileUrl } from '../lib/api'
import { useDeviceLive, useLocalState, usePolling } from '../lib/hooks'
import { PageHeader } from '../components/Chrome'
import { Panel, Badge, Progress, Empty, Segmented, Spinner } from '../components/ui'
import { ResponseCurve, FingerprintRadar, PcaPlot } from '../components/Charts'
import { PredictionCard, ModelPanel, StatusBadge } from '../components/ResultCard'
import { SESSION_STATUS, DEVICE_STATES } from '../lib/sensors'
import { fingerprintFromSamples, normalize, meanVectors } from '../lib/signal'
import { fmt } from '../lib/format'

const pad = (n) => String(n).padStart(2, '0')
const COFFEES = [...Array.from({ length: 18 }, (_, i) => `C${pad(i + 1)}`), 'R00']
const GROUPS = Array.from({ length: 18 }, (_, i) => `G${pad(i + 1)}`)
const ALIQUOTS = Array.from({ length: 10 }, (_, i) => `A${pad(i + 1)}`)
const BATCHES = ['B1', 'B2', 'B3', 'B4']

const INITIAL = {
  daq_id: '', batch_id: 'B1', group_id: 'G01', coffee_id: 'C01', aliquot_id: 'A01',
  species: 'Arabica', bean_state: 'Green', roast_level: '', mass_g: '10.0', operator_name: '',
  protocol_ver: 'SOP-1.0', run_order: '', warmup_min: '30', heating_s: '300', room_t: '', room_rh: '',
  divider_ratio: '', notes: '', duration_s: '500', sample_period_ms: '1000', dry_run: false,
}

function Field({ label, children, hint, className = '' }) {
  return (
    <label className={`block ${className}`}>
      <span className="label">{label}</span>
      {children}
      {hint && <span className="mt-1 block text-xs text-ink-faint">{hint}</span>}
    </label>
  )
}

const num = (v) => (v === '' || v == null ? null : Number(v))

const STEPS = [
  { key: 'queued', label: 'Perintah dikirim' },
  { key: 'recording', label: 'Merekam data sensor' },
  { key: 'uploading', label: 'Inferensi TinyML & kirim' },
  { key: 'completed', label: 'Tersimpan di database' },
]

export default function ClassificationPage() {
  const { device, deviceId, devices, setDeviceId, withOperator, toast, model } = useApp()
  const [form, setForm] = useLocalState('enose.form', INITIAL)
  const [active, setActive] = useState(() => sessionStorage.getItem('enose.activeMeasurement') || '')
  const [busy, setBusy] = useState(false)
  const [nextId, setNextId] = useState('')
  const [refMode, setRefMode] = useState('same')
  const live = useDeviceLive(deviceId, 1500)
  const dev = live.device || device
  const set = (k) => (e) => setForm((f) => ({ ...f, [k]: e.target.type === 'checkbox' ? e.target.checked : e.target.value }))

  // DAQ_ID default dari nomor perangkat (DAQ01 → DAQ1)
  const daqDefault = useMemo(() => (deviceId ? `DAQ${Number(deviceId.replace(/\D/g, '')) || ''}` : ''), [deviceId])
  const daq = form.daq_id || daqDefault

  useEffect(() => {
    if (form.coffee_id === 'R00') setForm((f) => ({ ...f, species: 'ref', bean_state: 'ref' }))
    else if (form.species === 'ref') setForm((f) => ({ ...f, species: 'Arabica', bean_state: 'Green' }))
  }, [form.coffee_id]) // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (!daq) return
    const q = new URLSearchParams({ daq_id: daq, batch_id: form.batch_id, coffee_id: form.coffee_id, aliquot_id: form.aliquot_id })
    api(`/measurements/next-id?${q}`).then((d) => setNextId(d.measurement_id)).catch(() => setNextId(''))
  }, [daq, form.batch_id, form.coffee_id, form.aliquot_id, active])

  const session = usePolling(active ? `/measurements/${encodeURIComponent(active)}` : null, 2000)
  const row = session.data
  const features = usePolling('/features', 20000)

  const isRecording = dev?.state === 'recording'

  // Ikuti rekaman yang sedang berjalan di perangkat, walau dimulai dari browser lain.
  useEffect(() => {
    const mid = dev?.measurement_id
    if ((dev?.state === 'recording' || dev?.state === 'uploading') && mid && mid !== active) {
      setActive(mid)
      sessionStorage.setItem('enose.activeMeasurement', mid)
    }
  }, [dev?.state, dev?.measurement_id]) // eslint-disable-line react-hooks/exhaustive-deps
  const ready = dev?.online && dev?.state === 'idle' && !dev?.pending_command
  const sensorsOk = dev?.sensors?.ads_a && dev?.sensors?.ads_b

  const start = () => withOperator(async () => {
    setBusy(true)
    try {
      const body = {
        device_id: deviceId, daq_id: daq, batch_id: form.batch_id, group_id: form.group_id, coffee_id: form.coffee_id,
        aliquot_id: form.aliquot_id, species: form.species, bean_state: form.bean_state, roast_level: form.roast_level || null,
        mass_g: num(form.mass_g), operator_name: form.operator_name || null, protocol_ver: form.protocol_ver || null,
        run_order: num(form.run_order), warmup_min: num(form.warmup_min), heating_s: num(form.heating_s),
        room_t: num(form.room_t), room_rh: num(form.room_rh), divider_ratio: num(form.divider_ratio), notes: form.notes || null,
        duration_s: num(form.duration_s), sample_period_ms: num(form.sample_period_ms), dry_run: !!form.dry_run,
      }
      const r = await api('/measurements/start', { method: 'POST', body, operator: true })
      setActive(r.measurement_id)
      sessionStorage.setItem('enose.activeMeasurement', r.measurement_id)
      toast(`Perintah rekam ${r.measurement_id} dikirim ke ${deviceId}`)
      // Aliquot berikutnya & run order otomatis naik untuk pengukuran selanjutnya.
      setForm((f) => ({
        ...f,
        aliquot_id: ALIQUOTS[Math.min(ALIQUOTS.indexOf(f.aliquot_id) + 1, ALIQUOTS.length - 1)],
        run_order: f.run_order ? String(Number(f.run_order) + 1) : f.run_order,
        notes: '',
      }))
    } finally {
      setBusy(false)
    }
  })

  const stop = () => withOperator(async () => {
    await api(`/devices/${encodeURIComponent(deviceId)}/commands`, { method: 'POST', body: { type: 'stop' }, operator: true })
    toast('Perintah hentikan rekaman dikirim', 'warn')
  })

  // Langkah yang sedang berjalan
  const stepIndex = (() => {
    const s = row?.status
    if (s === 'completed') return 4
    if (dev?.state === 'uploading' && dev?.measurement_id === active) return 2
    if (s === 'recording' || (isRecording && dev?.measurement_id === active)) return 1
    if (s === 'queued') return 0
    return -1
  })()
  const failed = ['failed', 'cancelled', 'interrupted'].includes(row?.status)

  // Sampel milik pengukuran yang sedang dipantau
  const samples = live.measurementId === active ? live.samples : []
  const [dbSamples, setDbSamples] = useState([])
  useEffect(() => {
    if (row?.status === 'completed' && !samples.length && active) {
      api(`/measurements/${encodeURIComponent(active)}/samples`).then(setDbSamples).catch(() => setDbSamples([]))
    } else setDbSamples([])
  }, [row?.status, active, samples.length])
  const curve = samples.length ? samples : dbSamples

  const fpA = useMemo(() => fingerprintFromSamples(curve) || (row?.result?.sensor_peak_abs ? normalize(row.result.sensor_peak_abs) : null), [curve, row])
  const refVec = useMemo(() => {
    const f = (features.data || []).filter((x) => x.measurement_id !== active && x.qc_flag !== 'REJECT')
    const pick = refMode === 'same' ? f.filter((x) => x.coffee_id === row?.coffee_id) : refMode === 'r00' ? f.filter((x) => x.coffee_id === 'R00') : f.filter((x) => x.coffee_id === refMode)
    return meanVectors(pick.map((x) => normalize(x.peak_abs)))
  }, [features.data, refMode, row?.coffee_id, active])
  const coffeeOptions = useMemo(() => [...new Set((features.data || []).map((f) => f.coffee_id).filter(Boolean))].sort(), [features.data])

  const progress = dev?.measurement_id === active && dev?.samples_total ? dev.samples_done / dev.samples_total : row?.status === 'completed' ? 1 : 0
  const elapsed = samples.length ? samples[samples.length - 1].t_s : 0
  const windowName = elapsed < 60 ? 'Baseline window' : elapsed < 400 ? 'Response window' : 'Late window'

  return (
    <>
      <PageHeader title="Klasifikasi kopi">
        Isi kartu identitas pengukuran, tekan Mulai rekam, lalu ESP32 menjalankan rantai lengkap: baca sensor → kumpulkan data → TinyML → simpan ke database.
      </PageHeader>
      <div className="mx-auto grid max-w-7xl gap-6 px-4 py-6 sm:px-6 xl:grid-cols-[420px_1fr]">
        <Panel title="Pengujian sampel baru" subtitle="Metadata wajib handbook bagian 17.3">
          <div className="space-y-4">
            <Field label="Perangkat (ESP32-S3)">
              <select className="field" value={deviceId} onChange={(e) => setDeviceId(e.target.value)}>
                {!devices.length && <option value="">Belum ada perangkat terhubung</option>}
                {devices.map((d) => <option key={d.device_id} value={d.device_id}>{d.device_id} — {d.online ? DEVICE_STATES[d.state]?.label || d.state : 'offline'}</option>)}
              </select>
            </Field>
            {dev && (
              <div className="flex flex-wrap gap-1.5">
                <Badge tone={dev.sensors?.ads_a ? 'ok' : 'danger'}>ADS1115 0x48</Badge>
                <Badge tone={dev.sensors?.ads_b ? 'ok' : 'danger'}>ADS1115 0x49</Badge>
                <Badge tone={dev.sensors?.dht ? 'ok' : 'warn'}>DHT22</Badge>
                {dev.state === 'warming_up' && <Badge tone="warn">Warm-up {dev.warmup_remaining_s}s</Badge>}
              </div>
            )}

            <div className="grid grid-cols-2 gap-3">
              <Field label="Coffee_ID">
                <select className="field" value={form.coffee_id} onChange={set('coffee_id')}>{COFFEES.map((c) => <option key={c}>{c}</option>)}</select>
              </Field>
              <Field label="Group_ID">
                <select className="field" value={form.group_id} onChange={set('group_id')}>{GROUPS.map((c) => <option key={c}>{c}</option>)}</select>
              </Field>
              <Field label="Species">
                <select className="field" value={form.species} onChange={set('species')} disabled={form.coffee_id === 'R00'}>
                  <option>Arabica</option><option>Robusta</option>{form.coffee_id === 'R00' && <option value="ref">ref</option>}
                </select>
              </Field>
              <Field label="Bean_State">
                <select className="field" value={form.bean_state} onChange={set('bean_state')} disabled={form.coffee_id === 'R00'}>
                  <option>Green</option><option>Roasted</option>{form.coffee_id === 'R00' && <option value="ref">ref</option>}
                </select>
              </Field>
              <Field label="Batch_ID">
                <select className="field" value={form.batch_id} onChange={set('batch_id')}>{BATCHES.map((c) => <option key={c}>{c}</option>)}</select>
              </Field>
              <Field label="Aliquot_ID">
                <select className="field" value={form.aliquot_id} onChange={set('aliquot_id')}>{ALIQUOTS.map((c) => <option key={c}>{c}</option>)}</select>
              </Field>
              <Field label="DAQ_ID"><input className="field" value={daq} onChange={set('daq_id')} placeholder="DAQ1" /></Field>
              <Field label="Massa aliquot (g)"><input className="field" inputMode="decimal" value={form.mass_g} onChange={set('mass_g')} /></Field>
              <Field label="Roast level">
                <select className="field" value={form.roast_level} onChange={set('roast_level')} disabled={form.bean_state !== 'Roasted'}>
                  <option value="">—</option><option>Light</option><option>Medium</option><option>Dark</option><option value="unknown">Tidak diketahui</option>
                </select>
              </Field>
              <Field label="Operator"><input className="field" value={form.operator_name} onChange={set('operator_name')} placeholder="Inisial" /></Field>
            </div>

            <details className="rounded-lg border border-line px-3 py-2">
              <summary className="cursor-pointer text-sm font-medium">Kondisi sesi & parameter rekaman</summary>
              <div className="mt-3 grid grid-cols-2 gap-3 pb-1">
                <Field label="Protocol_Ver"><input className="field" value={form.protocol_ver} onChange={set('protocol_ver')} /></Field>
                <Field label="Run_Order"><input className="field" inputMode="numeric" value={form.run_order} onChange={set('run_order')} /></Field>
                <Field label="Warm-up (menit)"><input className="field" inputMode="numeric" value={form.warmup_min} onChange={set('warmup_min')} /></Field>
                <Field label="Pemanasan (detik)"><input className="field" inputMode="numeric" value={form.heating_s} onChange={set('heating_s')} /></Field>
                <Field label="Suhu ruang (°C)"><input className="field" inputMode="decimal" value={form.room_t} onChange={set('room_t')} /></Field>
                <Field label="RH ruang (%)"><input className="field" inputMode="decimal" value={form.room_rh} onChange={set('room_rh')} /></Field>
                <Field label="Divider_Ratio"><input className="field" inputMode="decimal" value={form.divider_ratio} onChange={set('divider_ratio')} /></Field>
                <Field label="Durasi rekaman (s)" hint="Baku 500 s"><input className="field" inputMode="numeric" value={form.duration_s} onChange={set('duration_s')} /></Field>
                <Field label="Interval sampling (ms)" hint="Baku 1000 ms (1 Hz)" className="col-span-2"><input className="field" inputMode="numeric" value={form.sample_period_ms} onChange={set('sample_period_ms')} /></Field>
              </div>
            </details>
            <Field label="Catatan / anomali"><textarea className="field min-h-[64px]" value={form.notes} onChange={set('notes')} /></Field>

            <label className="flex items-start gap-2.5 rounded-lg bg-warn-light/60 p-3 text-sm">
              <input type="checkbox" className="mt-0.5 h-4 w-4 accent-roast" checked={!!form.dry_run} onChange={set('dry_run')} />
              <span><b>Mode uji tanpa sensor.</b> Untuk mencoba alur ESP32 → server → database sebelum sensor terpasang. Data otomatis bertanda QC REJECT dan tidak masuk dataset riset.</span>
            </label>

            <div className="rounded-lg bg-bench px-3 py-2 text-sm">
              <span className="text-ink-soft">Measurement_ID: </span><span className="num font-semibold">{nextId || '—'}</span>
            </div>

            {isRecording ? (
              <button className="btn-danger w-full py-3" onClick={stop}><Square className="h-4 w-4" />Hentikan rekaman</button>
            ) : (
              <button className="btn-primary w-full py-3 text-base" onClick={start} disabled={busy || !ready || (!sensorsOk && !form.dry_run)}>
                {busy ? <Spinner /> : <Play className="h-4 w-4" />}Mulai rekam
              </button>
            )}
            {!ready && !isRecording && (
              <p className="text-xs text-ink-soft">
                {!dev ? 'Menunggu ESP32 terhubung.' : !dev.online ? 'Perangkat offline.' : dev.state === 'warming_up' ? 'Tunggu warm-up sensor selesai.' : dev.pending_command ? 'Menunggu perangkat menerima perintah sebelumnya.' : `Perangkat sedang ${DEVICE_STATES[dev.state]?.label?.toLowerCase() || dev.state}.`}
              </p>
            )}
            {ready && !sensorsOk && !form.dry_run && <p className="text-xs text-danger">ADS1115 belum terdeteksi. Pasang sensor, atau aktifkan mode uji tanpa sensor.</p>}
          </div>
        </Panel>

        <div className="min-w-0 space-y-6">
          <Panel title="Proses pengukuran" subtitle={active || 'Belum ada pengukuran di sesi ini'} actions={row && <StatusBadge status={row.status} map={SESSION_STATUS} />}>
            {!active ? (
              <Empty icon={FlaskConical} title="Siap untuk pengukuran">Timbang 10 g aliquot, panaskan 5 menit sesuai SOP, tutup chamber, lalu tekan Mulai rekam.</Empty>
            ) : (
              <>
                <ol className="grid grid-cols-1 gap-3 sm:grid-cols-4">
                  {STEPS.map((s, i) => {
                    const done = stepIndex > i || stepIndex === 4
                    const now = stepIndex === i && !failed
                    return (
                      <li key={s.key} className={`flex items-center gap-2 rounded-lg border p-2.5 text-sm ${now ? 'border-signal bg-signal-light/50' : done ? 'border-bean/40 bg-bean-light/50' : 'border-line'}`}>
                        {done ? <CheckCircle2 className="h-4 w-4 shrink-0 text-bean" /> : now ? <Loader2 className="h-4 w-4 shrink-0 animate-spin text-signal" /> : <Circle className="h-4 w-4 shrink-0 text-ink-faint" />}
                        <span>{i + 1}. {s.label}</span>
                      </li>
                    )
                  })}
                </ol>
                {failed && <p className="mt-3 rounded-lg bg-danger-light p-3 text-sm text-danger">{SESSION_STATUS[row.status]?.label}: {row.error || 'tanpa keterangan'}</p>}
                <div className="mt-4">
                  <div className="mb-1 flex justify-between text-sm">
                    <span className="text-ink-soft">{stepIndex === 1 ? windowName : 'Progres'}</span>
                    <span className="num">{dev?.measurement_id === active ? `${dev.samples_done}/${dev.samples_total} sampel · ${fmt(elapsed, 0)} s` : row ? `${row.samples_received} sampel tersimpan` : ''}</span>
                  </div>
                  <Progress value={progress} tone={failed ? 'danger' : 'signal'} label="Progres pengukuran" />
                </div>
              </>
            )}
          </Panel>

          <Panel title="Kurva respons" subtitle="Diperbarui setiap ±5 detik selama perekaman">
            <ResponseCurve samples={curve} durationS={Number(row?.duration_s) || 500} />
          </Panel>

          <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
            <Panel title="Hasil klasifikasi">
              {row?.result ? (
                <>
                  <PredictionCard row={row} threshold={model?.confidence_threshold} />
                  <a className="btn-ghost mt-4 w-full" href={fileUrl(`/measurements/${encodeURIComponent(row.measurement_id)}/raw.csv`)}><Download className="h-4 w-4" />Unduh CSV mentah</a>
                </>
              ) : <Empty title="Menunggu hasil">Hasil inferensi muncul setelah 500 detik rekaman selesai.</Empty>}
              <div className="mt-4"><ModelPanel model={model} /></div>
            </Panel>
            <Panel title="Aroma profile comparison" subtitle="Fingerprint pengukuran ini vs rata-rata referensi">
              <div className="mb-2 flex flex-wrap items-center gap-2">
                <Segmented value={['same', 'r00'].includes(refMode) ? refMode : 'other'} onChange={(v) => setRefMode(v === 'other' ? coffeeOptions[0] || 'same' : v)}
                  options={[{ value: 'same', label: 'Coffee_ID sama' }, { value: 'r00', label: 'R00' }, { value: 'other', label: 'Lainnya' }]} />
                {!['same', 'r00'].includes(refMode) && (
                  <select className="field w-auto py-1" value={refMode} onChange={(e) => setRefMode(e.target.value)}>
                    {coffeeOptions.map((c) => <option key={c}>{c}</option>)}
                  </select>
                )}
              </div>
              <FingerprintRadar a={fpA} b={refVec} aLabel={active || 'Pengukuran ini'} bLabel={refMode === 'same' ? `Rerata ${row?.coffee_id || ''}` : refMode === 'r00' ? 'Rerata R00' : `Rerata ${refMode}`} />
              {!refVec && fpA && <p className="text-xs text-ink-faint">Belum ada pengukuran referensi lain untuk dibandingkan.</p>}
            </Panel>
          </div>

          <Panel title="Posisi dalam ruang PCA" subtitle="Eksploratif — PCA di-fit pada seluruh data, bukan bagian model klasifikasi">
            <PcaPlot features={features.data} colorBy="coffee_id" highlight={active} height={300} />
          </Panel>
        </div>
      </div>
    </>
  )
}
