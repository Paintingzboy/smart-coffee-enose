import { Loader2 } from 'lucide-react'

const TONES = {
  ok: 'bg-bean-light text-bean',
  live: 'bg-signal-light text-signal',
  warn: 'bg-warn-light text-warn',
  danger: 'bg-danger-light text-danger',
  neutral: 'bg-bench text-ink-soft',
  roast: 'bg-roast-light text-roast-dark',
}
const DOTS = { ok: 'bg-bean', live: 'bg-signal', warn: 'bg-warn', danger: 'bg-danger', neutral: 'bg-ink-faint', roast: 'bg-roast' }

export function Badge({ tone = 'neutral', children, dot = false, pulse = false, className = '' }) {
  return (
    <span className={`chip ${TONES[tone] || TONES.neutral} ${className}`}>
      {dot && <span className={`h-1.5 w-1.5 rounded-full ${DOTS[tone]} ${pulse ? 'pulse-dot' : ''}`} />}
      {children}
    </span>
  )
}

export function Panel({ title, subtitle, actions, children, className = '', bodyClass = '' }) {
  return (
    <section className={`panel min-w-0 ${className}`}>
      {(title || actions) && (
        <header className="flex flex-wrap items-start justify-between gap-3 border-b border-line px-4 py-3 sm:px-5">
          <div className="min-w-0">
            {title && <h2 className="panel-title">{title}</h2>}
            {subtitle && <p className="panel-sub mt-0.5">{subtitle}</p>}
          </div>
          {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
        </header>
      )}
      <div className={`panel-pad ${bodyClass}`}>{children}</div>
    </section>
  )
}

export function Empty({ icon: Icon, title, children, action }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-4 py-10 text-center">
      {Icon && <Icon className="h-8 w-8 text-ink-faint" aria-hidden />}
      <p className="font-medium text-ink">{title}</p>
      {children && <div className="max-w-md text-sm text-ink-soft">{children}</div>}
      {action}
    </div>
  )
}

export function Spinner({ className = 'h-4 w-4' }) {
  return <Loader2 className={`${className} animate-spin`} aria-hidden />
}

export function Progress({ value, tone = 'roast', label }) {
  const v = Math.max(0, Math.min(1, value || 0))
  const bar = { roast: 'bg-roast', signal: 'bg-signal', bean: 'bg-bean', warn: 'bg-warn', danger: 'bg-danger' }[tone]
  return (
    <div>
      <div className="h-2.5 w-full overflow-hidden rounded-full bg-bench" role="progressbar" aria-valuenow={Math.round(v * 100)} aria-valuemin={0} aria-valuemax={100} aria-label={label}>
        <div className={`h-full rounded-full ${bar} transition-[width] duration-500`} style={{ width: `${v * 100}%` }} />
      </div>
    </div>
  )
}

export function Segmented({ value, onChange, options, size = 'sm' }) {
  return (
    <div className="inline-flex rounded-lg border border-line bg-bench p-0.5" role="radiogroup">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          onClick={() => onChange(o.value)}
          className={`rounded-md px-2.5 ${size === 'sm' ? 'py-1 text-xs' : 'py-1.5 text-sm'} font-medium transition-colors ${
            value === o.value ? 'bg-white text-ink shadow-sm' : 'text-ink-soft hover:text-ink'
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  )
}

export function KV({ items, cols = 2 }) {
  return (
    <dl className={`grid gap-x-6 gap-y-2.5 text-sm ${cols === 3 ? 'sm:grid-cols-3' : cols === 1 ? '' : 'sm:grid-cols-2'}`}>
      {items.filter(Boolean).map(([k, v]) => (
        <div key={k} className="flex items-baseline justify-between gap-3 border-b border-dashed border-line pb-1.5">
          <dt className="text-ink-soft">{k}</dt>
          <dd className="num truncate text-right font-medium text-ink">{v ?? '—'}</dd>
        </div>
      ))}
    </dl>
  )
}

/** Sparkline kecil berbasis SVG (lebih ringan daripada chart penuh untuk 8 kartu). */
export function Sparkline({ values, color = '#2F6F8F', height = 28 }) {
  const v = (values || []).filter((x) => Number.isFinite(x))
  if (v.length < 2) return <div style={{ height }} className="rounded bg-bench/70" />
  const min = Math.min(...v), max = Math.max(...v), r = max - min || 1e-6
  const pts = v.map((x, i) => `${(i / (v.length - 1)) * 100},${height - 2 - ((x - min) / r) * (height - 4)}`).join(' ')
  return (
    <svg viewBox={`0 0 100 ${height}`} preserveAspectRatio="none" className="w-full" style={{ height }} aria-hidden>
      <polyline points={pts} fill="none" stroke={color} strokeWidth="1.6" vectorEffect="non-scaling-stroke" />
    </svg>
  )
}
