import { useEffect, useMemo, useState } from 'react'
import { Search, FileSpreadsheet, FileJson, FileText, Archive, X, ChevronLeft, ChevronRight, ArrowUpDown, Database, Download } from 'lucide-react'
import { useApp } from '../lib/app'
import { api, fileUrl } from '../lib/api'
import { usePolling } from '../lib/hooks'
import { PageHeader } from '../components/Chrome'
import { Panel, Empty, KV, Segmented, Spinner, Badge } from '../components/ui'
import { ResponseCurve, FingerprintRadar, PcaPlot } from '../components/Charts'
import { PredictionCard, StatusBadge, QcBadge } from '../components/ResultCard'
import { SESSION_STATUS } from '../lib/sensors'
import { dateTime, fmt, prettyClass } from '../lib/format'
import { fingerprintFromSamples, normalize, meanVectors } from '../lib/signal'

const EMPTY = { search: '', from: '', to: '', coffee_id: '', species: '', bean_state: '', device_id: '', batch_id: '', status: '', qc_flag: '' }

function useQuery(filters, page, sort, order) {
  return useMemo(() => {
    const q = new URLSearchParams()
    Object.entries(filters).forEach(([k, v]) => v && q.set(k, v))
    q.set('page', page); q.set('page_size', 20); q.set('sort', sort); q.set('order', order)
    return q.toString()
  }, [filters, page, sort, order])
}

function filterQs(filters) {
  const q = new URLSearchParams()
  Object.entries(filters).forEach(([k, v]) => v && q.set(k, v))
  return q.toString()
}

export default function AnalyticsPage() {
  const { devices, toast } = useApp()
  const [filters, setFilters] = useState(EMPTY)
  const [draft, setDraft] = useState(EMPTY)
  const [page, setPage] = useState(1)
  const [sort, setSort] = useState('time')
  const [order, setOrder] = useState('desc')
  const [selected, setSelected] = useState(null)
  const [colorBy, setColorBy] = useState('coffee_id')
  const [exporting, setExporting] = useState(false)
  const qs = useQuery(filters, page, sort, order)
  const list = usePolling(`/measurements?${qs}`, 15000)
  const stats = usePolling('/stats', 20000)
  const features = usePolling('/features', 30000)

  const apply = (e) => { e?.preventDefault(); setFilters(draft); setPage(1) }
  const reset = () => { setDraft(EMPTY); setFilters(EMPTY); setPage(1) }
  const sortBy = (k) => { if (sort === k) setOrder((o) => (o === 'asc' ? 'desc' : 'asc')); else { setSort(k); setOrder('desc') } }
  const d = (k) => (e) => setDraft((f) => ({ ...f, [k]: e.target.value }))
  const total = list.data?.total || 0
  const pages = Math.max(1, Math.ceil(total / 20))

  const exportExcel = async () => {
    setExporting(true)
    try {
      const q = new URLSearchParams(filterQs(filters)); q.set('page_size', 500)
      const data = await api(`/measurements?${q}`)
      const XLSX = await import('xlsx')
      const rows = data.items.map((r) => ({
        Measurement_ID: r.measurement_id, Device_ID: r.device_id, DAQ_ID: r.daq_id, Batch_ID: r.batch_id, Group_ID: r.group_id,
        Coffee_ID: r.coffee_id, Species: r.species, Bean_State: r.bean_state, Roast_Level: r.roast_level, Aliquot_ID: r.aliquot_id,
        Mass_g: r.mass_g, Mulai: r.started_at, Selesai: r.finished_at, Status: r.status, QC_Flag: r.qc_flag, Samples: r.samples_received,
        Prediksi: r.result?.predicted_class, Confidence: r.result?.confidence, Model: r.result?.model_version,
        Temperature: r.result?.temperature, Humidity: r.result?.humidity, Inference_ms: r.result?.inference_time_ms, Notes: r.notes,
        ...Object.fromEntries((r.result?.sensor_peak_abs || []).map((v, k) => [`peak_S${k + 1}`, v])),
        ...Object.fromEntries((r.result?.sensor_late_mean || []).map((v, k) => [`late_S${k + 1}`, v])),
      }))
      const ws = XLSX.utils.json_to_sheet(rows)
      const wb = XLSX.utils.book_new()
      XLSX.utils.book_append_sheet(wb, ws, 'Measurements')
      XLSX.writeFile(wb, `smart_coffee_enose_${new Date().toISOString().slice(0, 10)}.xlsx`)
    } catch (e) {
      toast(e.message, 'danger')
    } finally {
      setExporting(false)
    }
  }

  const fq = filterQs(filters)
  const contingency = (obj) => {
    const rows = Object.keys(obj || {}).sort()
    const cols = [...new Set(rows.flatMap((r) => Object.keys(obj[r])))].sort()
    return { rows, cols }
  }

  return (
    <>
      <PageHeader title="Analisis & riwayat">Seluruh pengukuran tim. Data mentah tidak pernah dihapus — pengukuran bermasalah ditandai QC (handbook 17.5).</PageHeader>
      <div className="mx-auto max-w-7xl space-y-6 px-4 py-6 sm:px-6">
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-[1fr_320px]">
          <Panel title="Filter & pencarian">
            <form onSubmit={apply} className="grid grid-cols-2 gap-3 md:grid-cols-4">
              <label className="col-span-2"><span className="label">Cari Measurement_ID / Coffee_ID / catatan</span>
                <div className="relative"><Search className="pointer-events-none absolute left-3 top-2.5 h-4 w-4 text-ink-faint" /><input className="field pl-9" value={draft.search} onChange={d('search')} placeholder="mis. C07 atau DAQ03_B02" /></div>
              </label>
              <label><span className="label">Dari tanggal</span><input type="date" className="field" value={draft.from} onChange={d('from')} /></label>
              <label><span className="label">Sampai tanggal</span><input type="date" className="field" value={draft.to} onChange={d('to')} /></label>
              <label><span className="label">Coffee_ID</span><input className="field" value={draft.coffee_id} onChange={d('coffee_id')} placeholder="C01–C18, R00" /></label>
              <label><span className="label">Spesies</span><select className="field" value={draft.species} onChange={d('species')}><option value="">Semua</option><option>Arabica</option><option>Robusta</option><option value="ref">ref (R00)</option></select></label>
              <label><span className="label">Kondisi biji</span><select className="field" value={draft.bean_state} onChange={d('bean_state')}><option value="">Semua</option><option>Green</option><option>Roasted</option></select></label>
              <label><span className="label">Perangkat</span><select className="field" value={draft.device_id} onChange={d('device_id')}><option value="">Semua</option>{devices.map((x) => <option key={x.device_id}>{x.device_id}</option>)}</select></label>
              <label><span className="label">Batch</span><select className="field" value={draft.batch_id} onChange={d('batch_id')}><option value="">Semua</option>{['B1', 'B2', 'B3', 'B4'].map((b) => <option key={b}>{b}</option>)}</select></label>
              <label><span className="label">Status</span><select className="field" value={draft.status} onChange={d('status')}><option value="">Semua</option>{Object.entries(SESSION_STATUS).map(([k, v]) => <option key={k} value={k}>{v.label}</option>)}</select></label>
              <label><span className="label">QC_Flag</span><select className="field" value={draft.qc_flag} onChange={d('qc_flag')}><option value="">Semua</option><option>OK</option><option>SUSPECT</option><option>REJECT</option></select></label>
              <div className="flex items-end gap-2"><button className="btn-primary flex-1">Terapkan</button><button type="button" className="btn-ghost" onClick={reset}>Reset</button></div>
            </form>
          </Panel>
          <Panel title="Ekspor data" subtitle="Mengikuti filter yang aktif">
            <div className="grid gap-2">
              <a className="btn-ghost justify-start" href={fileUrl(`/export/metadata.csv?${fq}`)}><FileText className="h-4 w-4 text-bean" />Metadata CSV (format handbook)</a>
              <button className="btn-ghost justify-start" onClick={exportExcel} disabled={exporting}>{exporting ? <Spinner /> : <FileSpreadsheet className="h-4 w-4 text-bean" />}Excel (.xlsx) + fitur</button>
              <a className="btn-ghost justify-start" href={fileUrl(`/export/measurements.json?${fq}`)}><FileJson className="h-4 w-4 text-signal" />JSON lengkap</a>
              <a className="btn-ghost justify-start" href={fileUrl(`/export/dataset.zip?${fq}`)}><Archive className="h-4 w-4 text-roast" />Dataset ZIP (raw + metadata + checksum)</a>
            </div>
            <p className="mt-3 text-xs text-ink-faint">ZIP berisi folder 01_raw_data/ dan 02_metadata/ siap pakai untuk struktur repositori proyek dan unggah ke Edge Impulse.</p>
          </Panel>
        </div>

        <Panel title="Riwayat pengukuran" subtitle={`${total} pengukuran`} bodyClass="!p-0">
          {list.loading && !list.data ? <div className="flex justify-center p-10"><Spinner className="h-6 w-6" /></div> : !total ? (
            <Empty icon={Database} title="Belum ada pengukuran">{list.error || 'Pengukuran dari halaman Klasifikasi Kopi akan tampil di sini.'}</Empty>
          ) : (
            <>
              <div className="overflow-x-auto">
                <table className="table-base">
                  <thead>
                    <tr>
                      {[['time', 'Waktu'], ['measurement_id', 'Measurement_ID'], ['coffee_id', 'Coffee'], [null, 'Spesies / kondisi'], ['device_id', 'DAQ'], [null, 'Batch'], ['status', 'Status'], [null, 'QC'], [null, 'Prediksi'], ['confidence', 'Confidence'], [null, '']].map(([k, label], i) => (
                        <th key={i}>{k ? <button className="inline-flex items-center gap-1 hover:text-ink" onClick={() => sortBy(k)}>{label}<ArrowUpDown className={`h-3 w-3 ${sort === k ? 'text-signal' : ''}`} /></button> : label}</th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {list.data.items.map((r) => (
                      <tr key={r.measurement_id} className="cursor-pointer hover:bg-bench/50" onClick={() => setSelected(r.measurement_id)}>
                        <td className="num text-ink-soft">{dateTime(r.started_at || r.created_at)}</td>
                        <td className="num font-medium">{r.measurement_id}{r.dry_run && <Badge tone="warn" className="ml-2">uji</Badge>}</td>
                        <td>{r.coffee_id}</td>
                        <td>{r.species} · {r.bean_state}</td>
                        <td>{r.device_id}</td>
                        <td>{r.batch_id}</td>
                        <td><StatusBadge status={r.status} map={SESSION_STATUS} /></td>
                        <td><QcBadge qc={r.qc_flag} /></td>
                        <td>{r.result ? prettyClass(r.result.predicted_class) : '—'}</td>
                        <td className="num">{fmt(r.result?.confidence, 3)}</td>
                        <td>
                          <a onClick={(e) => e.stopPropagation()} href={fileUrl(`/measurements/${encodeURIComponent(r.measurement_id)}/raw.csv`)} className="inline-flex items-center gap-1 text-signal hover:underline" title="Unduh CSV mentah">
                            <Download className="h-4 w-4" />CSV
                          </a>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <div className="flex items-center justify-between gap-3 px-4 py-3 text-sm">
                <span className="text-ink-soft">Halaman {page} dari {pages}</span>
                <div className="flex gap-2">
                  <button className="btn-ghost px-2.5" disabled={page <= 1} onClick={() => setPage((p) => p - 1)} aria-label="Sebelumnya"><ChevronLeft className="h-4 w-4" /></button>
                  <button className="btn-ghost px-2.5" disabled={page >= pages} onClick={() => setPage((p) => p + 1)} aria-label="Berikutnya"><ChevronRight className="h-4 w-4" /></button>
                </div>
              </div>
            </>
          )}
        </Panel>

        <Panel title="Uji diagnostik PCA" subtitle="Handbook 20.2: score plot yang sama diwarnai tiga cara. Kelompok rapi per Coffee_ID = sinyal aroma dominan; per DAQ/Batch = efek perangkat/sesi."
          actions={<Segmented value={colorBy} onChange={setColorBy} options={[{ value: 'coffee_id', label: 'Coffee_ID' }, { value: 'daq_id', label: 'DAQ_ID' }, { value: 'batch_id', label: 'Batch_ID' }, { value: 'species', label: 'Spesies' }]} />}>
          <PcaPlot features={features.data} colorBy={colorBy} />
          <p className="mt-2 text-xs text-ink-faint">Fitur: rerata late window + respons puncak 8 kanal (16 fitur, z-score). Hanya untuk eksplorasi; PCA dalam pipeline model wajib di-fit di dalam fold.</p>
        </Panel>

        <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
          {[['contingency_batch_species', 'Batch_ID × Species'], ['contingency_daq_species', 'DAQ_ID × Species']].map(([key, title]) => {
            const obj = stats.data?.[key]
            const { rows, cols } = contingency(obj)
            return (
              <Panel key={key} title={title} subtitle="Sel nol = kelas hanya muncul di sebagian batch/perangkat → LOBO/LODO tidak valid untuk kelas itu (handbook 15.2)">
                {!rows.length ? <Empty title="Belum ada pengukuran selesai" /> : (
                  <div className="overflow-x-auto">
                    <table className="table-base">
                      <thead><tr><th></th>{cols.map((c) => <th key={c} className="text-right">{c}</th>)}</tr></thead>
                      <tbody>{rows.map((r) => (
                        <tr key={r}><td className="font-medium">{r}</td>{cols.map((c) => {
                          const n = obj[r][c] || 0
                          return <td key={c} className={`num text-right ${n === 0 ? 'bg-danger-light font-semibold text-danger' : ''}`}>{n}</td>
                        })}</tr>
                      ))}</tbody>
                    </table>
                  </div>
                )}
              </Panel>
            )
          })}
        </div>

        <Panel title="Progres alokasi per Coffee_ID" subtitle="Target handbook 16.4: 10 aliquot per sample, tersebar di 3 DAQ dan 3 batch" bodyClass="!p-0">
          {!stats.data?.coffees?.length ? <Empty title="Belum ada data" /> : (
            <div className="overflow-x-auto">
              <table className="table-base">
                <thead><tr><th>Coffee_ID</th><th>Group</th><th>Spesies · kondisi</th><th>Aliquot selesai</th><th>DAQ</th><th>Batch</th></tr></thead>
                <tbody>{stats.data.coffees.map((c) => (
                  <tr key={c.coffee_id}>
                    <td className="font-medium">{c.coffee_id}</td><td>{c.group_id}</td><td>{c.species} · {c.bean_state}</td>
                    <td><div className="flex items-center gap-2"><div className="h-2 w-28 overflow-hidden rounded-full bg-bench"><div className="h-full bg-bean" style={{ width: `${Math.min(100, (c.aliquots.length / 10) * 100)}%` }} /></div><span className="num text-xs">{c.aliquots.length}/10</span></div></td>
                    <td className={c.devices.length < 3 ? 'text-warn' : ''}>{c.devices.join(', ')}</td>
                    <td className={c.batches.length < 3 ? 'text-warn' : ''}>{c.batches.join(', ')}</td>
                  </tr>
                ))}</tbody>
              </table>
            </div>
          )}
        </Panel>
      </div>
      {selected && <DetailDrawer id={selected} onClose={() => setSelected(null)} features={features.data} onChanged={list.refresh} />}
    </>
  )
}

function DetailDrawer({ id, onClose, features, onChanged }) {
  const { withOperator, toast, model } = useApp()
  const [row, setRow] = useState(null)
  const [samples, setSamples] = useState(null)
  const [qc, setQc] = useState('OK')
  const [notes, setNotes] = useState('')
  useEffect(() => {
    api(`/measurements/${encodeURIComponent(id)}`).then((r) => { setRow(r); setQc(r.qc_flag); setNotes(r.notes || '') }).catch((e) => toast(e.message, 'danger'))
    api(`/measurements/${encodeURIComponent(id)}/samples`).then(setSamples).catch(() => setSamples([]))
  }, [id, toast])
  useEffect(() => {
    const onKey = (e) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  const fp = useMemo(() => fingerprintFromSamples(samples) || (row?.result?.sensor_peak_abs ? normalize(row.result.sensor_peak_abs) : null), [samples, row])
  const ref = useMemo(() => meanVectors((features || []).filter((f) => f.coffee_id === row?.coffee_id && f.measurement_id !== id).map((f) => normalize(f.peak_abs))), [features, row, id])

  const saveQc = () => withOperator(async () => {
    await api(`/measurements/${encodeURIComponent(id)}/qc`, { method: 'PATCH', body: { qc_flag: qc, notes }, operator: true })
    toast(`QC ${id} disimpan sebagai ${qc}`)
    onChanged?.()
  })

  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-espresso/50" onClick={onClose} role="dialog" aria-modal="true" aria-label={`Detail ${id}`}>
      <div className="h-full w-full max-w-3xl overflow-y-auto bg-bench" onClick={(e) => e.stopPropagation()}>
        <div className="sticky top-0 z-10 flex items-center justify-between gap-3 bg-espresso px-5 py-4 text-white">
          <h2 className="num truncate font-display text-lg font-semibold">{id}</h2>
          <button onClick={onClose} className="rounded p-1 hover:bg-white/10" aria-label="Tutup"><X className="h-5 w-5" /></button>
        </div>
        {!row ? <div className="flex justify-center p-10"><Spinner className="h-6 w-6" /></div> : (
          <div className="space-y-5 p-4 sm:p-5">
            <Panel title="Metadata">
              <KV items={[
                ['Coffee_ID', row.coffee_id], ['Group_ID', row.group_id], ['Species', row.species], ['Bean_State', row.bean_state],
                ['Roast level', row.roast_level], ['Aliquot_ID', row.aliquot_id], ['DAQ_ID', row.daq_id], ['Device', row.device_id],
                ['Batch_ID', row.batch_id], ['Massa', row.mass_g != null ? `${fmt(row.mass_g, 2)} g` : null], ['Operator', row.operator_name],
                ['Protocol', row.protocol_ver], ['Run_Order', row.run_order], ['Warm-up', row.warmup_min != null ? `${row.warmup_min} menit` : null],
                ['Durasi', `${row.duration_s} s @ ${row.sample_period_ms} ms`], ['Sampel diterima', row.samples_received],
                ['Mulai', dateTime(row.started_at)], ['Selesai', dateTime(row.finished_at)],
                ['Status', SESSION_STATUS[row.status]?.label || row.status], ['Error', row.error],
              ]} />
            </Panel>
            <Panel title="Kurva respons"><ResponseCurve samples={samples || []} height={260} durationS={row.duration_s} emptyText="Data mentah tidak tersedia untuk pengukuran ini." /></Panel>
            <div className="grid grid-cols-1 gap-5 md:grid-cols-2">
              <Panel title="Hasil">{row.result ? <PredictionCard row={row} threshold={model?.confidence_threshold} compact /> : <Empty title="Tanpa hasil inferensi" />}</Panel>
              <Panel title="Fingerprint"><FingerprintRadar a={fp} b={ref} aLabel="Pengukuran ini" bLabel={`Rerata ${row.coffee_id}`} height={240} /></Panel>
            </div>
            <Panel title="Kontrol kualitas" subtitle="Menandai, bukan menghapus. Alasan wajib dicatat.">
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-[160px_1fr]">
                <label><span className="label">QC_Flag</span><select className="field" value={qc} onChange={(e) => setQc(e.target.value)}><option>OK</option><option>SUSPECT</option><option>REJECT</option></select></label>
                <label><span className="label">Catatan</span><input className="field" value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="mis. chamber terbuka di detik 210" /></label>
              </div>
              <div className="mt-3 flex flex-wrap gap-2">
                <button className="btn-primary" onClick={saveQc}>Simpan QC</button>
                <a className="btn-ghost" href={fileUrl(`/measurements/${encodeURIComponent(id)}/raw.csv`)}><Download className="h-4 w-4" />Unduh {id}.csv</a>
              </div>
            </Panel>
          </div>
        )}
      </div>
    </div>
  )
}
