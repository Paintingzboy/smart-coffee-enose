// Klien API. Default: origin yang sama (backend Rust juga menyajikan dashboard).
// Untuk dev terpisah, set VITE_API_URL=https://xxx.up.railway.app
export const API_BASE = (import.meta.env.VITE_API_URL || '').replace(/\/$/, '')

const KEY = 'enose.operatorKey'
export const getOperatorKey = () => sessionStorage.getItem(KEY) || ''
export const setOperatorKey = (k) => (k ? sessionStorage.setItem(KEY, k) : sessionStorage.removeItem(KEY))

export class ApiError extends Error {
  constructor(message, status) {
    super(message)
    this.status = status
  }
}

export async function api(path, { method = 'GET', body, form, operator = false, signal } = {}) {
  const headers = {}
  if (body !== undefined) headers['Content-Type'] = 'application/json'
  if (operator) headers['X-Operator-Key'] = getOperatorKey()
  let res
  try {
    res = await fetch(`${API_BASE}/api${path}`, {
      method,
      headers,
      body: form ?? (body !== undefined ? JSON.stringify(body) : undefined),
      signal,
    })
  } catch (e) {
    if (e.name === 'AbortError') throw e
    throw new ApiError('Server tidak dapat dihubungi. Periksa koneksi internet.', 0)
  }
  const text = await res.text()
  let data = null
  try { data = text ? JSON.parse(text) : null } catch { data = text }
  if (!res.ok) {
    if (res.status === 401 && operator) {
      setOperatorKey('')
      window.dispatchEvent(new CustomEvent('operator-required'))
    }
    throw new ApiError(data?.error || `Permintaan gagal (${res.status})`, res.status)
  }
  return data
}

export const fileUrl = (path) => `${API_BASE}/api${path}`
