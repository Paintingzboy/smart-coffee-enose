// PCA ringan (z-score + power iteration) untuk plot diagnostik handbook 20.2:
// score plot yang sama diwarnai menurut Coffee_ID, DAQ_ID, dan Batch_ID.
// Catatan: PCA di sini di-fit pada seluruh data → HANYA untuk eksplorasi visual,
// bukan bagian pipeline klasifikasi (yang wajib di-fit di dalam fold).

function zscore(X) {
  const n = X.length, p = X[0].length
  const mu = Array(p).fill(0), sd = Array(p).fill(0)
  for (const r of X) r.forEach((v, j) => (mu[j] += v / n))
  for (const r of X) r.forEach((v, j) => (sd[j] += (v - mu[j]) ** 2 / Math.max(1, n - 1)))
  return X.map((r) => r.map((v, j) => (sd[j] > 1e-12 ? (v - mu[j]) / Math.sqrt(sd[j]) : 0)))
}

function covariance(Z) {
  const n = Z.length, p = Z[0].length
  const C = Array.from({ length: p }, () => Array(p).fill(0))
  for (const r of Z) for (let i = 0; i < p; i++) for (let j = i; j < p; j++) C[i][j] += (r[i] * r[j]) / Math.max(1, n - 1)
  for (let i = 0; i < p; i++) for (let j = 0; j < i; j++) C[i][j] = C[j][i]
  return C
}

function powerIter(C, iters = 200) {
  const p = C.length
  let v = Array.from({ length: p }, (_, i) => 1 / Math.sqrt(p) + (i % 2 ? 1e-3 : -1e-3))
  let lambda = 0
  for (let it = 0; it < iters; it++) {
    const w = C.map((row) => row.reduce((a, c, j) => a + c * v[j], 0))
    const n = Math.sqrt(w.reduce((a, b) => a + b * b, 0)) || 1
    lambda = n
    v = w.map((x) => x / n)
  }
  return { v, lambda }
}

/** rows: array vektor fitur. Mengembalikan skor PC1/PC2 + rasio varians. */
export function pca2(rows) {
  if (!rows || rows.length < 3) return null
  const Z = zscore(rows)
  const C = covariance(Z)
  const total = C.reduce((a, r, i) => a + r[i], 0) || 1
  const a = powerIter(C)
  const C2 = C.map((r, i) => r.map((c, j) => c - a.lambda * a.v[i] * a.v[j]))
  const b = powerIter(C2)
  const scores = Z.map((r) => [r.reduce((s, x, j) => s + x * a.v[j], 0), r.reduce((s, x, j) => s + x * b.v[j], 0)])
  return { scores, explained: [a.lambda / total, b.lambda / total], loadings: [a.v, b.v] }
}
