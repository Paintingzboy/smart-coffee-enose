import { useState } from 'react'
import { Github, Linkedin } from 'lucide-react'
import { PageHeader } from '../components/Chrome'
import { Panel } from '../components/ui'
import { SITE, TEAM } from '../config/site'
import { SENSORS } from '../lib/sensors'

function Box({ x, y, w, h, title, lines = [], tone = '#241A15', fill = '#fff' }) {
  return (
    <g>
      <rect x={x} y={y} width={w} height={h} rx="10" fill={fill} stroke={tone} strokeWidth="1.5" />
      <text x={x + 14} y={y + 26} fontSize="15" fontWeight="600" fill={tone} fontFamily="IBM Plex Sans Condensed, sans-serif">{title}</text>
      {lines.map((l, i) => <text key={i} x={x + 14} y={y + 48 + i * 18} fontSize="12" fill="#5C5650" fontFamily="IBM Plex Sans, sans-serif">{l}</text>)}
    </g>
  )
}
function Arrow({ d, label, lx, ly, dashed }) {
  return (
    <g>
      <path d={d} fill="none" stroke="#8A857F" strokeWidth="1.5" strokeDasharray={dashed ? '5 4' : undefined} markerEnd="url(#arr)" />
      {label && <text x={lx} y={ly} fontSize="11" fill="#5C5650" textAnchor="middle" fontFamily="IBM Plex Sans, sans-serif">{label}</text>}
    </g>
  )
}

function Architecture() {
  return (
    <div className="overflow-x-auto">
      <svg viewBox="0 0 1080 420" className="min-w-[760px]" role="img" aria-label="Diagram arsitektur sistem">
        <defs><marker id="arr" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L10,5 L0,10 z" fill="#8A857F" /></marker></defs>
        <Box x={10} y={20} w={210} h={130} title="Chamber + sensor" lines={['8× MOS (MQ & TGS)', 'DHT22 (T, RH)', '2× ADS1115 16-bit (I²C)', 'Aliquot 10 g, pasif']} tone="#6E7F4E" />
        <Box x={10} y={200} w={210} h={150} title="ESP32-S3 (Rust)" lines={['Task akuisisi 1 Hz', 'Fitur + TinyML (Edge Impulse)', 'MQTT/TLS ke ThingsBoard', 'OTA dengan rollback']} tone="#9A5B2E" fill="#F3E6DA" />
        <Box x={310} y={170} w={220} h={140} title="ThingsBoard Cloud (EU)" lines={['Broker MQTT (port 8883)', 'Telemetry enose_* · RPC', 'Paket OTA firmware']} tone="#7A4E8C" fill="#EEE6F2" />
        <Box x={610} y={150} w={215} h={150} title="Backend Rust (Axum)" lines={['Railway — 1 URL publik', 'Jembatan TB (WebSocket)', 'QC, ekspor, antrian perintah', 'Menyajikan dashboard React']} tone="#2F6F8F" fill="#DEEAF0" />
        <Box x={880} y={30} w={190} h={110} title="Azure SQL" lines={['measurement_sessions', 'measurement_samples (raw)', 'sensor_readings (hasil)']} />
        <Box x={880} y={180} w={190} h={110} title="Dashboard React" lines={['PC & mobile', 'Live monitoring, riwayat', 'Klasifikasi, OTA']} />
        <Box x={610} y={335} w={215} h={70} title="Edge Impulse" lines={['Training → ekspor C++ library']} tone="#5B6472" />
        <Arrow d="M115 150 L115 196" label="analog → I²C" lx={170} ly={178} />
        <Arrow d="M220 230 L306 230" label="MQTT/TLS" lx={263} ly={221} />
        <Arrow d="M306 262 L224 262" label="RPC perintah" lx={263} ly={280} />
        <Arrow d="M306 300 L224 322" label="OTA (HTTP)" lx={263} ly={330} dashed />
        <Arrow d="M530 215 L606 215" label="WebSocket" lx={568} ly={206} />
        <Arrow d="M606 255 L534 255" label="RPC (REST)" lx={570} ly={273} />
        <Arrow d="M825 180 L876 100" label="SQL" lx={862} ly={150} />
        <Arrow d="M825 235 L876 235" label="REST" lx={851} ly={226} />
        <Arrow d="M606 385 C 430 425, 240 415, 140 354" label="model C++ (FFI)" lx={400} ly={402} dashed />
      </svg>
    </div>
  )
}

function Member({ m }) {
  const [img, setImg] = useState(true)
  const initials = m.name.split(' ').slice(0, 2).map((w) => w[0]).join('')
  return (
    <article className="flex gap-4 rounded-xl border border-line bg-white p-4">
      {img ? (
        <img src={`/assets/team/${m.photo}`} alt={`Foto ${m.name}`} onError={() => setImg(false)} className="h-20 w-20 shrink-0 rounded-xl object-cover" />
      ) : (
        <div className="flex h-20 w-20 shrink-0 items-center justify-center rounded-xl bg-roast-light font-display text-2xl font-semibold text-roast-dark" aria-hidden>{initials}</div>
      )}
      <div className="min-w-0">
        <h3 className="font-display text-base font-semibold leading-tight">{m.name}</h3>
        <p className="num text-sm text-ink-soft">NRP {m.nrp}</p>
        <p className="mt-1 text-sm">{m.role}</p>
        <div className="mt-2 flex gap-2">
          {m.github && <a href={m.github} target="_blank" rel="noreferrer" aria-label={`GitHub ${m.name}`} className="text-ink-soft hover:text-ink"><Github className="h-4 w-4" /></a>}
          {m.linkedin && <a href={m.linkedin} target="_blank" rel="noreferrer" aria-label={`LinkedIn ${m.name}`} className="text-ink-soft hover:text-ink"><Linkedin className="h-4 w-4" /></a>}
        </div>
      </div>
    </article>
  )
}

export default function AboutPage() {
  return (
    <>
      <PageHeader title="Tentang sistem & tim">Case-Based Project Smart Coffee Processing: IoT-Based Electronic Nose for Coffee Aroma Profiling and TinyML Classification.</PageHeader>
      <div className="mx-auto max-w-7xl space-y-6 px-4 py-6 sm:px-6">
        <Panel title="Arsitektur sistem" subtitle="ESP32 hanya membuat koneksi keluar (MQTT/TLS ke ThingsBoard), sehingga bekerja dari WiFi mana pun tanpa IP publik">
          <Architecture />
        </Panel>
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
          <Panel title="Spesifikasi hardware">
            <ul className="space-y-2 text-sm">
              <li><b>Mikrokontroler:</b> ESP32-S3 (WiFi, dual-core), firmware Embedded Rust (esp-idf)</li>
              <li><b>ADC:</b> 2× ADS1115 16-bit, I²C 0x48 & 0x49, PGA ±4,096 V (125 µV/LSB)</li>
              <li><b>Sensor lingkungan:</b> DHT22 (T & RH), dibaca ≤0,5 Hz, zero-order hold ke grid 1 Hz</li>
              <li><b>Chamber:</b> pasif/statis — tanpa pompa dan tanpa valve</li>
            </ul>
            <div className="mt-4 overflow-x-auto">
              <table className="table-base">
                <thead><tr><th>Kode</th><th>Sensor</th><th>Kanal ADC</th><th>Representative target gases</th></tr></thead>
                <tbody>{SENSORS.map((s) => <tr key={s.key}><td>{s.code}</td><td style={{ color: s.color }} className="font-medium">{s.name}</td><td>{s.adc}</td><td className="whitespace-normal">{s.targets}</td></tr>)}</tbody>
              </table>
            </div>
            <p className="mt-2 text-xs text-ink-faint">Sensor MOS tidak selektif; kolom target gas adalah sensitivitas nominal lembar data, bukan pengukuran konsentrasi.</p>
          </Panel>
          <Panel title="Spesifikasi software">
            <ul className="space-y-2 text-sm">
              <li><b>Firmware:</b> Rust (esp-idf-svc), task akuisisi terpisah dari jaringan, OTA dua partisi + rollback</li>
              <li><b>TinyML:</b> Edge Impulse (ekspor C++ library, dipanggil via FFI dari Rust)</li>
              <li><b>Backend:</b> Rust, Axum, Tokio, Tiberius (Azure SQL)</li>
              <li><b>Frontend:</b> React, Vite, Tailwind CSS, Recharts</li>
              <li><b>Cloud:</b> Railway (hosting), Azure SQL Database, ThingsBoard Cloud EU (broker MQTT + OTA)</li>
            </ul>
          </Panel>
        </div>
        <Panel title="Informasi akademis">
          <dl className="grid gap-4 text-sm sm:grid-cols-3">
            <div><dt className="text-ink-soft">Mata kuliah</dt><dd className="font-medium">{SITE.course}<br />{SITE.classLabel}</dd></div>
            <div><dt className="text-ink-soft">Departemen</dt><dd className="font-medium">{SITE.department}, {SITE.faculty}<br />{SITE.campus}</dd></div>
            <div><dt className="text-ink-soft">Dosen pengampu</dt><dd className="font-medium">{SITE.lecturer}</dd></div>
          </dl>
        </Panel>
        {TEAM.map((g) => (
          <Panel key={g.group} title={g.group}>
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">{g.members.map((m) => <Member key={m.nrp} m={m} />)}</div>
          </Panel>
        ))}
      </div>
    </>
  )
}
