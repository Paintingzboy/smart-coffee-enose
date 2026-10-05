import { useState } from 'react'
import { Cpu, Zap, RotateCcw, Upload, Rocket, RefreshCw, Gauge, ScrollText, CloudOff } from 'lucide-react'
import { useApp } from '../lib/app'
import { api } from '../lib/api'
import { useDeviceLive, usePolling } from '../lib/hooks'
import { PageHeader } from '../components/Chrome'
import { Panel, Badge, Empty, KV, Progress, Spinner } from '../components/ui'
import { SENSORS, DEVICE_STATES } from '../lib/sensors'
import { ago, bytes, dateTime, duration, fmt, timeOnly } from '../lib/format'
import { SITE } from '../config/site'

const FW_STATES = {
  DOWNLOADING: { label: 'Mengunduh firmware', tone: 'live', p: 0.35 },
  DOWNLOADED: { label: 'Unduhan selesai', tone: 'live', p: 0.7 },
  VERIFIED: { label: 'Checksum terverifikasi', tone: 'live', p: 0.8 },
  UPDATING: { label: 'Menulis & restart', tone: 'live', p: 0.9 },
  UPDATED: { label: 'Berhasil diperbarui', tone: 'ok', p: 1 },
  FAILED: { label: 'Gagal', tone: 'danger', p: 1 },
}

export default function DevicesPage() {
  const { devices, deviceId, setDeviceId, withOperator, toast, status } = useApp()
  const live = useDeviceLive(deviceId, 3000)
  const dev = live.device || devices.find((d) => d.device_id === deviceId)
  const ota = usePolling(deviceId ? `/ota/devices/${encodeURIComponent(deviceId)}` : null, 5000)
  const pkgs = usePolling(status?.thingsboard?.configured ? '/ota/packages' : null, 30000)
  const [period, setPeriod] = useState('')
  const [file, setFile] = useState(null)
  const [version, setVersion] = useState('')
  const [title, setTitle] = useState('')
  const [uploading, setUploading] = useState(false)

  const command = (type, extra = {}, msg) => withOperator(async () => {
    await api(`/devices/${encodeURIComponent(deviceId)}/commands`, { method: 'POST', body: { type, ...extra }, operator: true })
    toast(msg || `Perintah ${type} dikirim`)
  })

  const upload = (e) => {
    e.preventDefault()
    withOperator(async () => {
      setUploading(true)
      try {
        const form = new FormData()
        form.append('file', file)
        form.append('version', version.trim())
        form.append('device_id', deviceId)
        if (title.trim()) form.append('title', title.trim())
        form.append('assign', 'true')
        await api('/ota/upload', { method: 'POST', form, operator: true })
        toast(`Firmware ${version} diunggah ke ThingsBoard dan di-assign ke ${deviceId}`)
        setFile(null); setVersion('')
        pkgs.refresh(); ota.refresh()
      } finally {
        setUploading(false)
      }
    })
  }

  const assign = (pkg) => withOperator(async () => {
    await api('/ota/assign', { method: 'POST', body: { device_id: deviceId, package_id: pkg.id }, operator: true })
    toast(`${pkg.title} ${pkg.version} di-assign ke ${deviceId}`)
    ota.refresh()
  })

  const tbState = ota.data?.thingsboard?.telemetry?.fw_state?.value
  const fw = FW_STATES[tbState]
  const devOta = ota.data?.progress
  const otaProgress = devOta?.state && devOta.state !== 'IDLE' ? (devOta.progress ?? 0) : fw?.p ?? 0
  const st = DEVICE_STATES[dev?.state] || { label: dev?.state, tone: 'neutral' }
  const tbCfg = status?.thingsboard

  return (
    <>
      <PageHeader title="Perangkat & OTA">Detail ESP32-S3, kalibrasi sensor, dan pembaruan firmware jarak jauh melalui ThingsBoard.</PageHeader>
      <div className="mx-auto max-w-7xl space-y-6 px-4 py-6 sm:px-6">
        {!devices.length ? (
          <Panel><Empty icon={Cpu} title="Belum ada ESP32 yang terhubung">Flash firmware terbaru, nyalakan perangkat, dan pastikan WiFi serta URL server benar. Perangkat muncul otomatis.</Empty></Panel>
        ) : (
          <div className="flex gap-3 overflow-x-auto pb-1">
            {devices.map((d) => (
              <button key={d.device_id} onClick={() => setDeviceId(d.device_id)}
                className={`panel min-w-[180px] p-3 text-left ${d.device_id === deviceId ? 'border-roast ring-2 ring-roast/20' : ''}`}>
                <p className="font-display font-semibold">{d.device_id}</p>
                <p className="text-xs text-ink-soft">fw {d.fw_version} · {ago(d.seconds_since_seen)}</p>
                <Badge className="mt-2" tone={d.online ? (DEVICE_STATES[d.state]?.tone || 'neutral') : 'danger'} dot>{d.online ? DEVICE_STATES[d.state]?.label || d.state : 'Offline'}</Badge>
              </button>
            ))}
          </div>
        )}

        {dev && (
          <div className="grid gap-6 lg:grid-cols-2">
            <Panel title={`Detail ${dev.device_id}`} actions={<Badge tone={dev.online ? st.tone : 'danger'} dot>{dev.online ? st.label : 'Offline'}</Badge>}>
              <KV items={[
                ['IP address', dev.ip], ['MAC address', dev.mac], ['Firmware', `${dev.fw_title || ''} v${dev.fw_version}`],
                ['Uptime', duration(dev.uptime_s)], ['Sinyal WiFi', dev.rssi != null ? `${dev.rssi} dBm` : null],
                ['Heap bebas', bytes(dev.free_heap)], ['Heap minimum', bytes(dev.min_free_heap)],
                ['Interval sampling', dev.sample_period_ms ? `${dev.sample_period_ms} ms` : null],
                ['Model TinyML', dev.model_ready ? 'Terpasang' : 'Belum (capture only)'], ['Antrian unggah', dev.pending_uploads ?? 0],
                ['Pertama terlihat', dateTime(dev.first_seen)], ['Terakhir terlihat', ago(dev.seconds_since_seen)],
              ]} />
              <div className="mt-4 flex flex-wrap gap-2">
                <Badge tone={dev.sensors?.ads_a ? 'ok' : 'danger'} dot>ADS1115 #1 (0x48) · S1–S4</Badge>
                <Badge tone={dev.sensors?.ads_b ? 'ok' : 'danger'} dot>ADS1115 #2 (0x49) · S5–S8</Badge>
                <Badge tone={dev.sensors?.dht ? 'ok' : 'warn'} dot>DHT22 · GPIO4</Badge>
              </div>
              {dev.last_error && <p className="mt-3 rounded-lg bg-danger-light p-3 text-sm text-danger">{dev.last_error}</p>}
            </Panel>

            <Panel title="Kalibrasi & pengaturan">
              <div className="space-y-5">
                <div>
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div>
                      <p className="font-medium">Zero baseline udara bersih</p>
                      <p className="text-sm text-ink-soft">Rata-rata 10 detik pembacaan saat chamber kosong. Dipakai sebagai acuan ΔV di kartu sensor — tidak menggantikan baseline 0–60 s tiap pengukuran.</p>
                    </div>
                    <button className="btn-ghost" disabled={!dev.online || dev.state !== 'idle'} onClick={() => command('zero_baseline', {}, 'Perangkat merekam zero baseline (±10 detik)')}><Zap className="h-4 w-4 text-warn" />Set zero baseline</button>
                  </div>
                  {dev.zero_baseline && (
                    <div className="mt-3 grid grid-cols-4 gap-2 text-center text-xs sm:grid-cols-8">
                      {SENSORS.map((s, k) => (
                        <div key={s.key} className="rounded-md bg-bench p-1.5"><p className="text-ink-faint">{s.name}</p><p className="num font-medium">{fmt(dev.zero_baseline[k], 3)}</p></div>
                      ))}
                      <p className="col-span-full text-left text-ink-faint">Direkam {dateTime(dev.zero_baseline_at)}</p>
                    </div>
                  )}
                </div>
                <form className="flex flex-wrap items-end gap-2" onSubmit={(e) => { e.preventDefault(); command('set_config', { sample_period_ms: Number(period) }, `Interval sampling diatur ke ${period} ms`) }}>
                  <label className="flex-1"><span className="label">Interval sampling default (ms)</span><input className="field" inputMode="numeric" placeholder={String(dev.sample_period_ms || 1000)} value={period} onChange={(e) => setPeriod(e.target.value)} /></label>
                  <button className="btn-ghost" disabled={!period || !dev.online}><Gauge className="h-4 w-4" />Terapkan</button>
                </form>
                <p className="-mt-3 text-xs text-ink-faint">Baku handbook: 1000 ms (1 Hz). Ubah hanya untuk eksperimen laju sampling (mis. kelompok G02).</p>
                <div className="flex flex-wrap gap-2 border-t border-line pt-4">
                  <button className="btn-ghost" disabled={!dev.online || dev.state === 'recording'} onClick={() => window.confirm(`Restart ${dev.device_id}? Sensor akan warm-up ulang.`) && command('reboot', {}, 'Perintah restart dikirim')}><RotateCcw className="h-4 w-4" />Restart perangkat</button>
                  <button className="btn-ghost" disabled={!dev.online || dev.state !== 'idle'} onClick={() => command('check_ota', {}, 'Perangkat diminta memeriksa firmware baru')}><RefreshCw className="h-4 w-4" />Cek firmware sekarang</button>
                </div>
              </div>
            </Panel>

            <Panel title="ThingsBoard OTA manager" subtitle={tbCfg?.url || SITE.thingsboardUrl} className="lg:col-span-2"
              actions={<Badge tone={!tbCfg?.configured ? 'warn' : tbCfg.ok ? 'ok' : 'danger'} dot>{!tbCfg?.configured ? 'Belum dikonfigurasi' : tbCfg.ok ? 'Terhubung' : 'Gagal login'}</Badge>}>
              {!tbCfg?.configured ? (
                <Empty icon={CloudOff} title="ThingsBoard belum dikonfigurasi di server">Set variabel TB_URL dan TB_API_KEY (atau TB_USERNAME/TB_PASSWORD) di Railway, lalu redeploy.</Empty>
              ) : (
                <div className="grid gap-6 lg:grid-cols-2">
                  <div>
                    <KV cols={1} items={[
                      ['Versi berjalan di perangkat', ota.data?.running_version],
                      ['Device di ThingsBoard', ota.data?.thingsboard?.found ? ota.data.thingsboard.device.name : ota.data?.thingsboard?.error ? 'Tidak ditemukan' : '—'],
                      ['Versi dilaporkan ke ThingsBoard', ota.data?.thingsboard?.telemetry?.current_fw_version?.value],
                      ['Status OTA terakhir', fw ? `${fw.label} · ${timeOnly(ota.data.thingsboard.telemetry.fw_state.ts)}` : '—'],
                    ]} />
                    {ota.data?.thingsboard?.found === false && <p className="mt-2 text-sm text-danger">{ota.data.thingsboard.error}</p>}
                    <div className="mt-4">
                      <div className="mb-1 flex justify-between text-sm"><span className="text-ink-soft">{devOta?.state && devOta.state !== 'IDLE' ? devOta.message || devOta.state : fw?.label || 'Tidak ada pembaruan berjalan'}</span><span className="num">{fmt(otaProgress * 100, 0)}%</span></div>
                      <Progress value={otaProgress} tone={tbState === 'FAILED' ? 'danger' : tbState === 'UPDATED' ? 'bean' : 'signal'} label="Progres OTA" />
                    </div>
                    {ota.data?.thingsboard?.telemetry?.fw_error?.value && tbState === 'FAILED' && <p className="mt-2 text-sm text-danger">{ota.data.thingsboard.telemetry.fw_error.value}</p>}
                  </div>
                  <form onSubmit={upload} className="space-y-3 rounded-xl border border-dashed border-line p-4">
                    <p className="font-medium">Unggah firmware baru</p>
                    <label className="block"><span className="label">File firmware (.bin hasil espflash save-image)</span>
                      <input type="file" accept=".bin" className="field file:mr-3 file:rounded-md file:border-0 file:bg-bench file:px-3 file:py-1" onChange={(e) => setFile(e.target.files?.[0] || null)} /></label>
                    <div className="grid grid-cols-2 gap-3">
                      <label><span className="label">Versi baru</span><input className="field" placeholder="mis. 1.0.1" value={version} onChange={(e) => setVersion(e.target.value)} /></label>
                      <label><span className="label">Judul (fw_title)</span><input className="field" placeholder={status?.fw_title} value={title} onChange={(e) => setTitle(e.target.value)} /></label>
                    </div>
                    <p className="text-xs text-ink-faint">Versi harus sama dengan <code>version</code> di Cargo.toml firmware yang di-build, dan judul harus sama dengan FW_TITLE firmware. OTA ditunda otomatis bila perangkat sedang merekam.</p>
                    <button className="btn-primary w-full" disabled={!file || !version.trim() || uploading}>{uploading ? <Spinner /> : <Rocket className="h-4 w-4" />}Unggah & push update</button>
                  </form>
                  <div className="lg:col-span-2">
                    <p className="mb-2 font-medium">Paket firmware di ThingsBoard</p>
                    {pkgs.loading && !pkgs.data ? <Spinner /> : !pkgs.data?.length ? <p className="text-sm text-ink-soft">{pkgs.error || 'Belum ada paket.'}</p> : (
                      <div className="overflow-x-auto">
                        <table className="table-base">
                          <thead><tr><th>Judul</th><th>Versi</th><th>File</th><th>Ukuran</th><th>Dibuat</th><th></th></tr></thead>
                          <tbody>{pkgs.data.map((p) => (
                            <tr key={p.id}>
                              <td>{p.title}</td><td className="num font-medium">{p.version}</td><td>{p.file_name || '—'}</td><td className="num">{bytes(p.data_size)}</td>
                              <td className="num">{p.created_time ? dateTime(p.created_time) : '—'}</td>
                              <td><button className="btn-ghost py-1" disabled={!p.has_data} onClick={() => assign(p)}><Upload className="h-3.5 w-3.5" />Assign ke {deviceId}</button></td>
                            </tr>
                          ))}</tbody>
                        </table>
                      </div>
                    )}
                  </div>
                </div>
              )}
            </Panel>

            <Panel title="Log aktivitas perangkat" className="lg:col-span-2" bodyClass="!p-0">
              {!live.events.length ? <Empty icon={ScrollText} title="Belum ada aktivitas" /> : (
                <ul className="max-h-80 divide-y divide-line overflow-y-auto">
                  {live.events.map((e, i) => (
                    <li key={i} className="flex flex-wrap gap-x-4 gap-y-1 px-5 py-2.5 text-sm">
                      <span className="num w-20 text-ink-faint">{timeOnly(e.at)}</span>
                      <Badge tone={e.event === 'error' || e.event === 'failed' || e.event === 'interrupted' ? 'danger' : e.event === 'completed' || e.event === 'result' ? 'ok' : 'neutral'}>{e.event}</Badge>
                      <span className="min-w-0 flex-1 text-ink-soft">{e.measurement_id && <b className="num mr-2 text-ink">{e.measurement_id}</b>}{e.message}</span>
                    </li>
                  ))}
                </ul>
              )}
            </Panel>
          </div>
        )}
      </div>
    </>
  )
}
