import { useState } from 'react'
import { Menu, X, Lock, Unlock, Cpu } from 'lucide-react'
import { useApp } from '../lib/app'
import { setOperatorKey } from '../lib/api'
import { SITE } from '../config/site'

export const ROUTES = [
  { path: '/dashboard', label: 'Dashboard' },
  { path: '/analytics', label: 'Analisis & Riwayat' },
  { path: '/classification', label: 'Klasifikasi Kopi' },
  { path: '/devices', label: 'Perangkat & OTA' },
  { path: '/about', label: 'Tentang' },
]

export function Logo({ className = 'h-9 w-9' }) {
  return <img src="/assets/logo-enose.png" alt="" className={className} onError={(e) => (e.currentTarget.style.visibility = 'hidden')} />
}

export default function Navbar({ route }) {
  const [open, setOpen] = useState(false)
  const { devices, deviceId, setDeviceId, hasKey, setHasKey, setOperatorOpen, toast } = useApp()

  const lockToggle = () => {
    if (hasKey) {
      setOperatorKey('')
      setHasKey(false)
      toast('Mode operator dikunci')
    } else setOperatorOpen(true)
  }

  return (
    <header className="sticky top-0 z-40 bg-espresso text-white" style={{ paddingTop: 'env(safe-area-inset-top)' }}>
      <div className="mx-auto flex h-16 max-w-7xl items-center gap-3 px-4 sm:px-6">
        <a href="#/dashboard" className="flex min-w-0 items-center gap-2.5">
          <Logo />
          <span className="min-w-0 leading-tight">
            <span className="block truncate font-display text-[17px] font-semibold">{SITE.title}</span>
            <span className="hidden truncate text-xs text-white/60 sm:block">{SITE.department} ITS</span>
          </span>
        </a>

        <nav className="ml-6 hidden items-center gap-1 lg:flex" aria-label="Menu utama">
          {ROUTES.map((r) => (
            <a
              key={r.path}
              href={`#${r.path}`}
              aria-current={route === r.path ? 'page' : undefined}
              className={`rounded-md px-3 py-2 text-sm transition-colors ${route === r.path ? 'bg-white/10 text-white' : 'text-white/70 hover:text-white'}`}
            >
              {r.label}
            </a>
          ))}
        </nav>

        <div className="ml-auto flex items-center gap-2">
          {devices.length > 0 && (
            <label className="hidden items-center gap-2 rounded-lg bg-white/10 px-2.5 py-1.5 text-sm sm:flex">
              <Cpu className="h-4 w-4 text-white/60" aria-hidden />
              <span className="sr-only">Perangkat aktif</span>
              <select value={deviceId} onChange={(e) => setDeviceId(e.target.value)} className="bg-transparent text-white focus:outline-none [&>option]:text-ink">
                {devices.map((d) => (
                  <option key={d.device_id} value={d.device_id}>
                    {d.device_id} {d.online ? '● online' : '○ offline'}
                  </option>
                ))}
              </select>
            </label>
          )}
          <button
            onClick={lockToggle}
            className={`rounded-lg p-2 ${hasKey ? 'bg-bean/30 text-white' : 'text-white/70 hover:bg-white/10'}`}
            title={hasKey ? 'Mode operator aktif — klik untuk mengunci' : 'Masuk mode operator'}
            aria-label={hasKey ? 'Kunci mode operator' : 'Masuk mode operator'}
          >
            {hasKey ? <Unlock className="h-5 w-5" /> : <Lock className="h-5 w-5" />}
          </button>
          <button className="rounded-lg p-2 text-white/80 hover:bg-white/10 lg:hidden" onClick={() => setOpen((o) => !o)} aria-label="Buka menu" aria-expanded={open}>
            {open ? <X className="h-6 w-6" /> : <Menu className="h-6 w-6" />}
          </button>
        </div>
      </div>

      {open && (
        <nav className="border-t border-white/10 px-4 pb-4 lg:hidden" aria-label="Menu utama (mobile)">
          {ROUTES.map((r) => (
            <a
              key={r.path}
              href={`#${r.path}`}
              onClick={() => setOpen(false)}
              className={`block rounded-md px-3 py-3 text-base ${route === r.path ? 'bg-white/10 text-white' : 'text-white/75'}`}
            >
              {r.label}
            </a>
          ))}
          {devices.length > 0 && (
            <label className="mt-3 flex items-center gap-2 rounded-lg bg-white/10 px-3 py-2.5 text-sm sm:hidden">
              <Cpu className="h-4 w-4 text-white/60" aria-hidden />
              <select value={deviceId} onChange={(e) => setDeviceId(e.target.value)} className="w-full bg-transparent text-white focus:outline-none [&>option]:text-ink">
                {devices.map((d) => (
                  <option key={d.device_id} value={d.device_id}>{d.device_id} {d.online ? '● online' : '○ offline'}</option>
                ))}
              </select>
            </label>
          )}
        </nav>
      )}
    </header>
  )
}
