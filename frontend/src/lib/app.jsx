import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react'
import { getOperatorKey } from './api'
import { usePolling } from './hooks'

const Ctx = createContext(null)

export function AppProvider({ children }) {
  const status = usePolling('/status', 4000)
  const [deviceId, setDeviceIdRaw] = useState(() => localStorage.getItem('enose.device') || '')
  const [toasts, setToasts] = useState([])
  const [operatorOpen, setOperatorOpen] = useState(false)
  const [hasKey, setHasKey] = useState(() => !!getOperatorKey())
  const pending = useRef(null)

  const devices = useMemo(
    () => [...(status.data?.devices || [])].sort((a, b) => a.device_id.localeCompare(b.device_id)),
    [status.data],
  )

  const setDeviceId = useCallback((id) => {
    setDeviceIdRaw(id)
    localStorage.setItem('enose.device', id)
  }, [])

  // Pilih otomatis perangkat online pertama bila belum ada pilihan.
  useEffect(() => {
    if (!devices.length) return
    if (!deviceId || !devices.some((d) => d.device_id === deviceId)) {
      setDeviceId((devices.find((d) => d.online) || devices[0]).device_id)
    }
  }, [devices, deviceId, setDeviceId])

  const toast = useCallback((message, tone = 'ok') => {
    const id = Math.random().toString(36).slice(2)
    setToasts((t) => [...t, { id, message, tone }])
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), tone === 'danger' ? 7000 : 4000)
  }, [])

  // Jalankan aksi yang butuh kunci operator; minta kunci dulu bila belum ada.
  const withOperator = useCallback(async (action) => {
    if (!getOperatorKey()) {
      pending.current = action
      setOperatorOpen(true)
      return
    }
    try {
      return await action()
    } catch (e) {
      if (e.status === 401) {
        setHasKey(false)
        pending.current = action
        setOperatorOpen(true)
      } else {
        toast(e.message, 'danger')
      }
    }
  }, [toast])

  useEffect(() => {
    const onReq = () => setHasKey(false)
    window.addEventListener('operator-required', onReq)
    return () => window.removeEventListener('operator-required', onReq)
  }, [])

  const onOperatorSaved = useCallback(() => {
    setHasKey(true)
    setOperatorOpen(false)
    const act = pending.current
    pending.current = null
    if (act) withOperator(act)
  }, [withOperator])

  const value = {
    status: status.data,
    statusError: status.error,
    refreshStatus: status.refresh,
    devices,
    device: devices.find((d) => d.device_id === deviceId) || null,
    deviceId,
    setDeviceId,
    model: status.data?.model,
    toast,
    toasts,
    withOperator,
    hasKey,
    setHasKey,
    operatorOpen,
    setOperatorOpen,
    onOperatorSaved,
  }
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}

export const useApp = () => useContext(Ctx)
