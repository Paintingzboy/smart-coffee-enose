import { useState } from 'react'
import { KeyRound, X, Github, MapPin } from 'lucide-react'
import { useApp } from '../lib/app'
import { api, setOperatorKey } from '../lib/api'
import { SITE } from '../config/site'
import { Spinner } from './ui'
import { Logo } from './Navbar'

/** Header ringkas halaman selain dashboard (memakai latar yang sama dengan hero). */
export function PageHeader({ title, children }) {
  return (
    <div className="hero-bg text-white">
      <div className="mx-auto max-w-7xl px-4 py-8 sm:px-6 sm:py-10">
        <h1 className="font-display text-3xl font-semibold sm:text-4xl">{title}</h1>
        {children && <p className="mt-2 max-w-2xl text-white/75">{children}</p>}
      </div>
    </div>
  )
}

export function OperatorDialog() {
  const { operatorOpen, setOperatorOpen, onOperatorSaved } = useApp()
  const [key, setKey] = useState('')
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState('')
  if (!operatorOpen) return null

  const submit = async (e) => {
    e.preventDefault()
    setBusy(true)
    setErr('')
    setOperatorKey(key.trim())
    try {
      await api('/auth/check', { method: 'POST', operator: true })
      setKey('')
      onOperatorSaved()
    } catch (e2) {
      setOperatorKey('')
      setErr(e2.status === 401 ? 'Kunci operator salah.' : e2.message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center bg-espresso/60 p-4 sm:items-center" role="dialog" aria-modal="true" aria-labelledby="op-title">
      <form onSubmit={submit} className="panel w-full max-w-sm p-5">
        <div className="mb-3 flex items-center justify-between">
          <h2 id="op-title" className="flex items-center gap-2 panel-title"><KeyRound className="h-5 w-5 text-roast" /> Mode operator</h2>
          <button type="button" onClick={() => setOperatorOpen(false)} className="rounded p-1 text-ink-soft hover:bg-bench" aria-label="Tutup"><X className="h-5 w-5" /></button>
        </div>
        <p className="mb-4 text-sm text-ink-soft">Merekam, kalibrasi, OTA, dan mengubah QC hanya bisa dilakukan operator. Pengunjung lain tetap bisa melihat data.</p>
        <label className="label" htmlFor="opkey">Kunci operator</label>
        <input id="opkey" type="password" autoFocus className="field" value={key} onChange={(e) => setKey(e.target.value)} autoComplete="current-password" />
        {err && <p className="mt-2 text-sm text-danger">{err}</p>}
        <button className="btn-primary mt-4 w-full" disabled={!key.trim() || busy}>{busy && <Spinner />} Masuk</button>
        <p className="mt-3 text-xs text-ink-faint">Kunci tersimpan hanya selama tab ini terbuka.</p>
      </form>
    </div>
  )
}

export function Toasts() {
  const { toasts } = useApp()
  const tone = { ok: 'border-bean', danger: 'border-danger', warn: 'border-warn', live: 'border-signal' }
  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-0 z-50 flex flex-col items-center gap-2 p-4 sm:items-end" style={{ paddingBottom: 'calc(1rem + env(safe-area-inset-bottom))' }} aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={`pointer-events-auto w-full max-w-sm rounded-lg border-l-4 bg-white px-4 py-3 text-sm shadow-lg ${tone[t.tone] || tone.ok}`}>
          {t.message}
        </div>
      ))}
    </div>
  )
}

export function Footer() {
  return (
    <footer className="mt-12 bg-espresso text-white/70" style={{ paddingBottom: 'env(safe-area-inset-bottom)' }}>
      <div className="mx-auto grid max-w-7xl gap-8 px-4 py-10 sm:px-6 md:grid-cols-3">
        <div className="flex items-start gap-3">
          <Logo className="h-10 w-10" />
          <div>
            <p className="font-display text-lg font-semibold text-white">{SITE.title}</p>
            <p className="text-sm">{SITE.department}, {SITE.faculty}<br />{SITE.campus}</p>
          </div>
        </div>
        <div className="text-sm">
          <p className="mb-2 font-medium text-white">Mata kuliah</p>
          <p>{SITE.course}</p>
          <p>{SITE.classLabel}</p>
          <p>Dosen pengampu: {SITE.lecturer}</p>
        </div>
        <div className="space-y-2 text-sm">
          <p className="mb-2 font-medium text-white">Tautan</p>
          <a href={SITE.repoUrl} target="_blank" rel="noreferrer" className="flex items-center gap-2 hover:text-white"><Github className="h-4 w-4" /> Repository GitHub</a>
          <p className="flex items-start gap-2"><MapPin className="mt-0.5 h-4 w-4 shrink-0" /> {SITE.labContact}</p>
        </div>
      </div>
      <div className="border-t border-white/10">
        <p className="mx-auto max-w-7xl px-4 py-4 text-xs sm:px-6">
          © 2026 Smart Coffee E-Nose — Departemen Teknik Instrumentasi ITS. Dibangun dengan Rust (Axum), React, Azure SQL, dan ThingsBoard. Di-host di Railway.
        </p>
      </div>
    </footer>
  )
}
