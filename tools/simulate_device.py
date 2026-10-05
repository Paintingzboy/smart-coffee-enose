#!/usr/bin/env python3
"""
Simulator ESP32-S3 E-Nose — meniru protokol firmware persis (heartbeat, perintah,
data mentah, event, hasil). Dipakai untuk menguji dashboard tanpa hardware.

Contoh:
  python tools/simulate_device.py --url http://localhost:3000 --key dev-device-key --id DAQ01
  python tools/simulate_device.py --url https://APP.up.railway.app --key <DEVICE_API_KEY> --id DAQ02 --fast

--fast  : 1 detik pengukuran disimulasikan setiap 0,05 detik (rekaman 500 s selesai ±25 s)
--no-sensors : laporkan ADS1115 tidak terdeteksi (meniru ESP32 tanpa sensor)
Data yang dihasilkan SINTETIS — jangan dicampur dengan dataset riset.
"""
import argparse, json, math, random, time, urllib.request, urllib.error
from datetime import datetime, timezone

p = argparse.ArgumentParser()
p.add_argument("--url", default="http://localhost:3000")
p.add_argument("--key", default="dev-device-key")
p.add_argument("--id", default="DAQ01")
p.add_argument("--fast", action="store_true")
p.add_argument("--no-sensors", action="store_true")
p.add_argument("--warmup", type=int, default=5)
a = p.parse_args()

FW = "1.0.0-sim"
SPEED = 0.05 if a.fast else 1.0
boot = time.time()
state = {"state": "warming_up", "mid": None, "done": 0, "total": 0, "last_ack": None,
         "err": None, "period": 1000, "stop": False}
rng = random.Random(a.id)
BASE = [rng.uniform(0.6, 1.4) for _ in range(8)]

def now(): return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

def post(path, body):
    req = urllib.request.Request(a.url.rstrip("/") + path, data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json", "X-Device-Key": a.key})
    try:
        with urllib.request.urlopen(req, timeout=15) as r:
            return json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        print("HTTP", e.code, path, e.read()[:200]); return None
    except Exception as e:
        print("ERR", path, e); return None

def coffee_gain(coffee):
    r = random.Random(coffee or "x")
    return [r.uniform(0.2, 1.6) for _ in range(8)]

def live():
    if a.no_sensors: return {"v": [None]*8, "temp_c": None, "rh_pct": None}
    return {"v": [round(b + rng.gauss(0, 0.003), 4) for b in BASE], "temp_c": round(28.5 + rng.gauss(0, .1), 1),
            "rh_pct": round(62 + rng.gauss(0, .3), 1)}

def heartbeat():
    hb = {"device_id": a.id, "fw_title": "smart-coffee-enose", "fw_version": FW, "ip": "192.168.1.50",
          "mac": "A4:CF:12:89:5B:C0", "rssi": rng.randint(-70, -55), "uptime_s": int(time.time() - boot),
          "free_heap": 180000 + rng.randint(0, 5000), "min_free_heap": 150000, "state": state["state"],
          "warmup_remaining_s": max(0, a.warmup - int(time.time() - boot)), "measurement_id": state["mid"],
          "samples_done": state["done"], "samples_total": state["total"],
          "sensors": {"ads_a": not a.no_sensors, "ads_b": not a.no_sensors, "dht": not a.no_sensors},
          "live": live(), "last_ack": state["last_ack"], "last_error": state["err"],
          "ota": {"state": "IDLE", "progress": 0}, "sample_period_ms": state["period"], "model_ready": True,
          "pending_uploads": 0}
    return post("/api/device/heartbeat", hb)

def record(cmd):
    m = cmd["measurement"]; mid = m["measurement_id"]
    period = cmd.get("sample_period_ms") or 1000
    n = int((cmd.get("duration_s") or 500) * 1000 / period)
    missing = a.no_sensors
    if missing and not cmd.get("allow_missing_sensors"):
        post("/api/device/event", {"device_id": a.id, "measurement_id": mid, "event": "failed",
                                   "message": "ADS1115 0x48 tidak terdeteksi"}); return
    state.update(state="recording", mid=mid, done=0, total=n, stop=False)
    post("/api/device/event", {"device_id": a.id, "measurement_id": mid, "event": "started"})
    gain = coffee_gain(m["coffee_id"]); tau = [rng.uniform(70, 160) for _ in range(8)]
    rows, all_v, last_hb = [], [], time.time()
    for i in range(n):
        if state["stop"]:
            post("/api/device/event", {"device_id": a.id, "measurement_id": mid, "event": "cancelled",
                                       "message": "Dihentikan operator"}); break
        t = i * period / 1000
        v = []
        for k in range(8):
            if missing: v.append(0.0); continue
            resp = 0 if t < 60 else gain[k] * 0.35 * (1 - math.exp(-(t - 60) / tau[k]))
            v.append(round(BASE[k] + resp + rng.gauss(0, 0.004), 6))
        raw = [int(x * 32768 / 4.096) for x in v]
        rows.append({"i": i, "t_s": round(t, 3), "ts": now(), "raw": raw, "v": v,
                     "temp_c": None if missing else round(28.6 + t / 500 * .4, 1),
                     "rh_pct": None if missing else round(62.0 - t / 500, 1), "dht_age_s": i % 2})
        all_v.append(v); state["done"] = i + 1
        if len(rows) >= 5:
            post("/api/device/samples", {"device_id": a.id, "measurement_id": mid, "rows": rows}); rows = []
        if time.time() - last_hb > 3: hb_cycle(); last_hb = time.time()
        time.sleep(period / 1000 * SPEED)
    else:
        if rows: post("/api/device/samples", {"device_id": a.id, "measurement_id": mid, "rows": rows})
        state["state"] = "uploading"
        base = [sum(r[k] for r in all_v[:60]) / 60 for k in range(8)]
        late = [sum(r[k] for r in all_v[-100:]) / 100 for k in range(8)]
        peak = [max(abs(r[k] - base[k]) for r in all_v) for k in range(8)]
        label = f"{m['species']}_{m['bean_state']}".lower() if not missing else "CAPTURE_ONLY"
        conf = round(rng.uniform(0.55, 0.97), 3)
        post("/api/readings", {"device_id": a.id, "fw_version": FW, **m, "predicted_class": label,
             "confidence": conf, "model_version": "sim-mlp-v1", "temperature": 28.8, "humidity": 61.5,
             "sample_count": n, "sample_duration_s": n * period / 1000, "inference_time_ms": round(rng.uniform(40, 90), 1),
             "sensor_late_mean": late, "sensor_peak_abs": peak, "seq": int(time.time()), "timestamp": now()})
        post("/api/device/event", {"device_id": a.id, "measurement_id": mid, "event": "completed"})
    state.update(state="idle", mid=None, done=0, total=0)

def hb_cycle():
    r = heartbeat()
    cmd = (r or {}).get("command")
    if cmd and cmd["id"] != state["last_ack"]:
        state["last_ack"] = cmd["id"]; print("CMD", cmd["type"], cmd.get("measurement", {}).get("measurement_id", ""))
        if cmd["type"] == "stop": state["stop"] = True
        elif cmd["type"] == "set_config": state["period"] = cmd.get("sample_period_ms") or 1000
        elif cmd["type"] == "zero_baseline":
            post("/api/device/event", {"device_id": a.id, "event": "zero_baseline", "values": BASE,
                                       "message": "Baseline udara bersih direkam"})
        elif cmd["type"] == "start" and state["state"] == "idle":
            return cmd
    return None

print(f"Simulator {a.id} → {a.url}")
while True:
    if state["state"] == "warming_up" and time.time() - boot >= a.warmup: state["state"] = "idle"
    cmd = hb_cycle()
    if cmd: hb_cycle(); record(cmd)  # ack dulu, lalu rekam
    time.sleep(3 if not a.fast else 1)
