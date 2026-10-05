// Array sensor sesuai handbook bagian 9.1. "targets" = representative target gases
// (sensitivitas nominal lembar data), BUKAN daftar senyawa yang diukur secara selektif.
export const SENSORS = [
  { key: 'mq3', code: 'S1', name: 'MQ-3', adc: 'ADS1115 #1 · A0', targets: 'Benzena, alkohol, aldehida', color: '#9A5B2E' },
  { key: 'mq6', code: 'S2', name: 'MQ-6', adc: 'ADS1115 #1 · A1', targets: 'CO, LPG, metana', color: '#6E7F4E' },
  { key: 'mq7', code: 'S3', name: 'MQ-7', adc: 'ADS1115 #1 · A2', targets: 'Aseton, metanol, alkohol', color: '#2F6F8F' },
  { key: 'mq135', code: 'S4', name: 'MQ-135', adc: 'ADS1115 #1 · A3', targets: 'VOC campuran (CO₂, CO, amonia)', color: '#B7791F' },
  { key: 'tgs2600', code: 'S5', name: 'TGS2600', adc: 'ADS1115 #2 · A0', targets: 'Etanol, hidrogen, isobutana', color: '#7A4E8C' },
  { key: 'tgs2602', code: 'S6', name: 'TGS2602', adc: 'ADS1115 #2 · A1', targets: 'Amonia, toluena, VOC berbau', color: '#B23A2E' },
  { key: 'tgs2611', code: 'S7', name: 'TGS2611', adc: 'ADS1115 #2 · A2', targets: 'Metana, isobutana, hidrogen', color: '#3E8E7E' },
  { key: 'tgs2620', code: 'S8', name: 'TGS2620', adc: 'ADS1115 #2 · A3', targets: 'Alkohol, uap pelarut', color: '#5B6472' },
]

/** FSR ADS1115 yang dipakai firmware (PGA ±4,096 V). */
export const ADC_FSR_V = 4.096
/** Di atas nilai ini dianggap mendekati saturasi (clipping) — perlu cek FSR/pembagi tegangan. */
export const NEAR_SATURATION_V = 3.9

/** Jendela waktu baku handbook 11.2 */
export const WINDOWS = [
  { name: 'Baseline', from: 0, to: 60, fill: '#2F6F8F' },
  { name: 'Response', from: 60, to: 400, fill: '#9A5B2E' },
  { name: 'Late', from: 400, to: 500, fill: '#6E7F4E' },
]

export const DEVICE_STATES = {
  booting: { label: 'Booting', tone: 'neutral' },
  warming_up: { label: 'Warm-up sensor', tone: 'warn' },
  idle: { label: 'Siap merekam', tone: 'ok' },
  recording: { label: 'Merekam', tone: 'live' },
  uploading: { label: 'Mengirim hasil', tone: 'live' },
  ota: { label: 'Update firmware', tone: 'warn' },
  error: { label: 'Error', tone: 'danger' },
}

export const SESSION_STATUS = {
  queued: { label: 'Menunggu perangkat', tone: 'warn' },
  recording: { label: 'Merekam', tone: 'live' },
  completed: { label: 'Selesai', tone: 'ok' },
  failed: { label: 'Gagal', tone: 'danger' },
  cancelled: { label: 'Dibatalkan', tone: 'neutral' },
  interrupted: { label: 'Terputus', tone: 'danger' },
}
