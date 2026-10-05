import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from './api'

/** Polling sederhana; berhenti sementara saat tab tidak terlihat. */
export function usePolling(path, ms = 5000, { enabled = true } = {}) {
  const [data, setData] = useState(null)
  const [error, setError] = useState(null)
  const [loading, setLoading] = useState(true)
  const tick = useRef(0)

  const load = useCallback(async () => {
    if (!path) return
    const my = ++tick.current
    try {
      const d = await api(path)
      if (my === tick.current) { setData(d); setError(null) }
    } catch (e) {
      if (my === tick.current) setError(e.message)
    } finally {
      if (my === tick.current) setLoading(false)
    }
  }, [path])

  useEffect(() => {
    if (!enabled || !path) return
    setLoading(true)
    let timer
    let alive = true
    const run = async () => {
      if (!document.hidden) await load()
      if (alive && ms) timer = setTimeout(run, ms)
    }
    run()
    return () => { alive = false; clearTimeout(timer) }
  }, [load, ms, enabled, path])

  return { data, error, loading, refresh: load }
}

/**
 * Data live satu perangkat: status + data mentah pengukuran aktif (inkremental).
 * Bila Measurement_ID berganti, buffer dikosongkan dan diambil ulang dari awal.
 */
export function useDeviceLive(deviceId, ms = 2000) {
  const [state, setState] = useState({ device: null, samples: [], history: [], events: [], measurementId: null, error: null })
  const buf = useRef({ mid: null, last: -1, samples: [] })

  useEffect(() => {
    buf.current = { mid: null, last: -1, samples: [] }
    setState({ device: null, samples: [], history: [], events: [], measurementId: null, error: null })
    if (!deviceId) return
    let alive = true
    let timer
    const path = (since) => `/devices/${encodeURIComponent(deviceId)}/live?since=${since}`
    const tick = async () => {
      if (!document.hidden) {
        try {
          let d = await api(path(buf.current.last))
          if (d.samples_measurement !== buf.current.mid) {
            if (buf.current.last > -1) d = await api(path(-1))
            buf.current = { mid: d.samples_measurement, last: -1, samples: [] }
          }
          const merged = buf.current.samples.concat(d.samples || [])
          buf.current.samples = merged
          buf.current.last = merged.length ? merged[merged.length - 1].i : -1
          if (alive) setState({ device: d.device, samples: merged, history: d.live_history || [], events: d.events || [], measurementId: d.samples_measurement, error: null })
        } catch (e) {
          if (alive) setState((s) => ({ ...s, error: e.message, device: e.status === 404 ? null : s.device }))
        }
      }
      if (alive) timer = setTimeout(tick, ms)
    }
    tick()
    return () => { alive = false; clearTimeout(timer) }
  }, [deviceId, ms])

  return state
}

export function useLocalState(key, initial) {
  const [v, setV] = useState(() => {
    try { const s = localStorage.getItem(key); return s ? { ...initial, ...JSON.parse(s) } : initial } catch { return initial }
  })
  useEffect(() => { try { localStorage.setItem(key, JSON.stringify(v)) } catch { /* abaikan */ } }, [key, v])
  return [v, setV]
}
