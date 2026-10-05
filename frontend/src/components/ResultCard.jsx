import { AlertTriangle, BrainCircuit, Info } from 'lucide-react'
import { fmt, pct, prettyClass, dateTime } from '../lib/format'
import { Badge, Progress, KV } from './ui'

/**
 * Hasil satu inferensi. Sesuai handbook 28.2: yang ditampilkan per prediksi adalah
 * CONFIDENCE (keluaran model untuk satu masukan), bukan akurasi.
 */
export function PredictionCard({ row, threshold = 0.6, compact = false }) {
  const r = row?.result
  if (!r) return null
  const captureOnly = !r.predicted_class || r.predicted_class === 'CAPTURE_ONLY'
  const low = !captureOnly && (r.confidence ?? 0) < threshold
  return (
    <div>
      <p className="text-xs text-ink-soft">Prediksi</p>
      <p className={`font-display font-semibold ${compact ? 'text-2xl' : 'text-3xl'} ${captureOnly ? 'text-ink-soft' : 'text-ink'}`}>{prettyClass(r.predicted_class)}</p>
      {captureOnly ? (
        <p className="mt-2 flex items-start gap-2 text-sm text-ink-soft"><Info className="mt-0.5 h-4 w-4 shrink-0" />Data mentah tersimpan. Klasifikasi aktif setelah model Edge Impulse ditanam ke firmware.</p>
      ) : (
        <div className="mt-3">
          <div className="mb-1 flex items-baseline justify-between text-sm">
            <span className="text-ink-soft">Confidence</span>
            <span className="num font-semibold">{fmt(r.confidence, 3)}</span>
          </div>
          <Progress value={r.confidence} tone={low ? 'warn' : 'bean'} label="Confidence prediksi" />
          {low && (
            <p className="mt-2 flex items-start gap-2 text-sm text-warn"><AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />Confidence di bawah ambang {fmt(threshold, 2)} — perlakukan sebagai tidak pasti.</p>
          )}
        </div>
      )}
      {!compact && (
        <div className="mt-4">
          <KV items={[
            ['Measurement_ID', row.measurement_id],
            ['Coffee_ID', row.coffee_id],
            ['Waktu inferensi', r.inference_time_ms ? `${fmt(r.inference_time_ms, 1)} ms` : '—'],
            ['Selesai', dateTime(row.finished_at || row.created_at)],
          ]} cols={1} />
        </div>
      )}
    </div>
  )
}

export function ModelPanel({ model }) {
  return (
    <div className="rounded-xl border border-line bg-bench/50 p-4">
      <p className="flex items-center gap-2 text-sm font-medium text-ink"><BrainCircuit className="h-4 w-4 text-signal" />Informasi model</p>
      <KV cols={1} items={[
        ['Model', model?.name],
        ['Versi', model?.version],
        ['Kinerja tervalidasi', model?.metric || 'Belum divalidasi'],
        model?.validation ? ['Skema validasi', model.validation] : null,
      ]} />
      <p className="mt-2 text-xs text-ink-faint">
        Kinerja model (macro-F1, GroupKFold) adalah sifat model dari validasi berlabel. Confidence pada setiap prediksi bukan akurasi, dan confidence tinggi tidak menjamin prediksi benar.
      </p>
    </div>
  )
}

export function StatusBadge({ status, map }) {
  const s = map[status] || { label: status, tone: 'neutral' }
  return <Badge tone={s.tone} dot pulse={s.tone === 'live'}>{s.label}</Badge>
}

export function QcBadge({ qc }) {
  const tone = qc === 'OK' ? 'ok' : qc === 'SUSPECT' ? 'warn' : 'danger'
  return <Badge tone={tone}>{qc}</Badge>
}

export { pct }
