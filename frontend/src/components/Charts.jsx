import { useMemo, useState } from 'react'
import {
  ResponsiveContainer, LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ReferenceArea,
  RadarChart, PolarGrid, PolarAngleAxis, PolarRadiusAxis, Radar, Legend, ScatterChart, Scatter, ZAxis,
} from 'recharts'
import { SENSORS, WINDOWS } from '../lib/sensors'
import { curveData } from '../lib/signal'
import { fmt } from '../lib/format'
import { Segmented, Empty } from './ui'
import { Activity } from 'lucide-react'
import { pca2 } from '../lib/pca'

const MODES = [
  { value: 'delta', label: 'ΔV' },
  { value: 'frac', label: 'S = ΔV/x₀' },
  { value: 'v', label: 'V mentah' },
]

export function ResponseCurve({ samples, height = 320, emptyText, durationS = 500 }) {
  const [mode, setMode] = useState('delta')
  const [hidden, setHidden] = useState(() => new Set())
  const data = useMemo(() => curveData(samples, mode), [samples, mode])
  const maxT = Math.max(durationS, data.length ? data[data.length - 1].t : 0)
  const unit = mode === 'frac' ? '' : 'V'
  const toggle = (k) => setHidden((h) => { const n = new Set(h); n.has(k) ? n.delete(k) : n.add(k); return n })

  return (
    <div>
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <Segmented value={mode} onChange={setMode} options={MODES} />
        <div className="flex flex-wrap gap-1">
          {SENSORS.map((s, k) => (
            <button key={s.key} type="button" onClick={() => toggle(k)} aria-pressed={!hidden.has(k)}
              className={`chip border ${hidden.has(k) ? 'border-line bg-white text-ink-faint line-through' : 'border-transparent bg-bench text-ink'}`}>
              <span className="h-2 w-2 rounded-full" style={{ background: s.color }} />{s.name}
            </button>
          ))}
        </div>
      </div>
      {!data.length ? (
        <Empty icon={Activity} title="Belum ada kurva respons">{emptyText || 'Kurva muncul begitu perangkat mulai merekam.'}</Empty>
      ) : (
        <div style={{ height }}>
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={data} margin={{ top: 18, right: 8, bottom: 4, left: 0 }}>
              <CartesianGrid stroke="#E4E5E0" strokeDasharray="3 3" />
              {WINDOWS.map((w) => (
                <ReferenceArea key={w.name} x1={w.from} x2={Math.min(w.to, maxT)} fill={w.fill} fillOpacity={0.05}
                  label={{ value: `${w.name} ${w.from}–${w.to} s`, position: 'insideTop', fontSize: 11, fill: '#8A857F' }} />
              ))}
              <XAxis dataKey="t" type="number" domain={[0, maxT]} tick={{ fontSize: 11 }} unit=" s" />
              <YAxis tick={{ fontSize: 11 }} width={54} tickFormatter={(v) => (mode === 'frac' ? fmt(v, 2) : fmt(v, 2))} />
              <Tooltip
                formatter={(v, n) => [Number.isFinite(v) ? `${fmt(v, mode === 'frac' ? 4 : 4)} ${unit}` : '—', SENSORS[Number(n.slice(1))]?.name]}
                labelFormatter={(t) => `t = ${fmt(t, 0)} s`}
                contentStyle={{ borderRadius: 10, borderColor: '#D9DBD3', fontSize: 12 }}
              />
              {SENSORS.map((s, k) => !hidden.has(k) && (
                <Line key={s.key} dataKey={`c${k}`} stroke={s.color} dot={false} strokeWidth={1.6} isAnimationActive={false} connectNulls />
              ))}
            </LineChart>
          </ResponsiveContainer>
        </div>
      )}
      <p className="mt-2 text-xs text-ink-faint">
        Jendela waktu baku handbook: baseline 0–60 s (x₀ = median), response 60–400 s, late 400–500 s. Sistem chamber pasif — tanpa fase purging.
      </p>
    </div>
  )
}

export function FingerprintRadar({ a, b, aLabel = 'Sampel', bLabel = 'Referensi', height = 300 }) {
  if (!a) return <Empty icon={Activity} title="Fingerprint belum tersedia">Muncul setelah ada respons sensor.</Empty>
  const data = SENSORS.map((s, k) => ({ axis: s.name, a: a[k] ?? 0, b: b?.[k] ?? null }))
  return (
    <div style={{ height }}>
      <ResponsiveContainer width="100%" height="100%">
        <RadarChart data={data} outerRadius="72%">
          <PolarGrid stroke="#D9DBD3" />
          <PolarAngleAxis dataKey="axis" tick={{ fontSize: 11, fill: '#5C5650' }} />
          <PolarRadiusAxis domain={[0, 1]} tick={false} axisLine={false} />
          <Radar name={aLabel} dataKey="a" stroke="#9A5B2E" fill="#9A5B2E" fillOpacity={0.28} isAnimationActive={false} />
          {b && <Radar name={bLabel} dataKey="b" stroke="#2F6F8F" fill="#2F6F8F" fillOpacity={0.08} strokeDasharray="5 4" isAnimationActive={false} />}
          {b && <Legend wrapperStyle={{ fontSize: 12 }} />}
          <Tooltip formatter={(v) => fmt(v, 3)} contentStyle={{ borderRadius: 10, fontSize: 12 }} />
        </RadarChart>
      </ResponsiveContainer>
    </div>
  )
}

const PALETTE = ['#9A5B2E', '#2F6F8F', '#6E7F4E', '#B7791F', '#7A4E8C', '#B23A2E', '#3E8E7E', '#5B6472', '#C07A54', '#4C8FB5', '#8FA35E', '#D4A23C', '#9C6FB0', '#D06A5E', '#5FB3A0', '#8790A0', '#6B4423', '#1F4F66']

/** Score plot PCA diagnostik (handbook 20.2) — diwarnai per Coffee_ID / DAQ_ID / Batch_ID. */
export function PcaPlot({ features, colorBy = 'coffee_id', highlight, height = 340 }) {
  const result = useMemo(() => {
    if (!features || features.length < 3) return null
    const rows = features.map((f) => [...f.late_mean, ...f.peak_abs])
    const p = pca2(rows)
    if (!p) return null
    const groups = {}
    features.forEach((f, i) => {
      const g = f[colorBy] || '—'
      ;(groups[g] ||= []).push({ x: p.scores[i][0], y: p.scores[i][1], id: f.measurement_id, g, hl: f.measurement_id === highlight })
    })
    return { groups, explained: p.explained }
  }, [features, colorBy, highlight])

  if (!result) return <Empty icon={Activity} title="Data belum cukup untuk PCA">Butuh minimal 3 pengukuran selesai dengan hasil fitur.</Empty>
  const names = Object.keys(result.groups).sort()
  const hl = names.flatMap((g) => result.groups[g]).filter((p) => p.hl)
  return (
    <div>
      <div style={{ height }}>
        <ResponsiveContainer width="100%" height="100%">
          <ScatterChart margin={{ top: 8, right: 12, bottom: 18, left: 0 }}>
            <CartesianGrid stroke="#E4E5E0" strokeDasharray="3 3" />
            <XAxis dataKey="x" type="number" name="PC1" tick={{ fontSize: 11 }} label={{ value: `PC1 (${fmt(result.explained[0] * 100, 1)}%)`, position: 'insideBottom', offset: -8, fontSize: 11 }} />
            <YAxis dataKey="y" type="number" name="PC2" tick={{ fontSize: 11 }} width={44} label={{ value: `PC2 (${fmt(result.explained[1] * 100, 1)}%)`, angle: -90, position: 'insideLeft', fontSize: 11 }} />
            <ZAxis range={[46, 46]} />
            <Tooltip cursor={{ strokeDasharray: '3 3' }} content={({ payload }) => {
              const p = payload?.[0]?.payload
              return p ? <div className="rounded-lg border border-line bg-white px-3 py-2 text-xs shadow"><b>{p.id}</b><br />{colorBy}: {p.g}</div> : null
            }} />
            {names.map((g, i) => (
              <Scatter key={g} name={g} data={result.groups[g]} fill={PALETTE[i % PALETTE.length]} fillOpacity={0.85} isAnimationActive={false} />
            ))}
            {hl.length > 0 && <Scatter name="Pengukuran ini" data={hl} fill="none" stroke="#241A15" strokeWidth={2.5} shape="circle" isAnimationActive={false} />}
          </ScatterChart>
        </ResponsiveContainer>
      </div>
      <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1">
        {names.map((g, i) => (
          <span key={g} className="flex items-center gap-1.5 text-xs text-ink-soft"><span className="h-2.5 w-2.5 rounded-full" style={{ background: PALETTE[i % PALETTE.length] }} />{g}</span>
        ))}
      </div>
    </div>
  )
}
