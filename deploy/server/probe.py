#!/usr/bin/env python3
"""Records one sample of how the site is doing, once a minute.

Each sample is a line of JSON in ~/linvesther/metrics/samples-YYYYMMDD.jsonl:
whether each public address answers (through the tunnel, as a visitor would
reach it), how long it took, which services and the database are up, the
tunnel's request counters, and the machine's load, memory and disk. A gap in
the samples means the machine (or its network) was off. report.py reads them.
"""
import json, os, re, shutil, subprocess, sys, time, urllib.request
from datetime import datetime, timezone
from pathlib import Path

HOME = Path.home()
OUT = HOME / "linvesther" / "metrics"
KEEP_DAYS = 90
TUNNEL_METRICS = "http://127.0.0.1:20241/metrics"
PUBLIC = {
    "site": "https://linvesther.com/",
    "app": "https://app.linvesther.com/",
    "docs": "https://docs.linvesther.com/",
    "api": "https://api.linvesther.com/healthz",
}
LOCAL = {"api_local": "http://127.0.0.1:4301/healthz", "web_local": "http://127.0.0.1:4300/"}
UNITS = ["linvesther-api", "linvesther-web", "linvesther-tunnel"]


def fetch(url, timeout=10):
    start = time.monotonic()
    try:
        with urllib.request.urlopen(urllib.request.Request(url, headers={"user-agent": "linvesther-probe"}), timeout=timeout) as r:
            status = r.status
    except urllib.error.HTTPError as e:
        status = e.code
    except Exception:
        status = 0
    return {"status": status, "ms": round((time.monotonic() - start) * 1000)}


def unit_active(name):
    r = subprocess.run(["systemctl", "--user", "is-active", name + ".service"], capture_output=True, text=True)
    return r.stdout.strip() == "active"


def database_healthy():
    r = subprocess.run(["docker", "inspect", "-f", "{{.State.Health.Status}}", "linvesther-postgres-1"], capture_output=True, text=True)
    return r.stdout.strip() == "healthy"


def tunnel_counters():
    try:
        with urllib.request.urlopen(TUNNEL_METRICS, timeout=5) as r:
            text = r.read().decode()
    except Exception:
        return None
    total, errors, codes = None, None, {}
    for line in text.splitlines():
        if line.startswith("cloudflared_tunnel_total_requests "):
            total = float(line.split()[-1])
        elif line.startswith("cloudflared_tunnel_request_errors "):
            errors = float(line.split()[-1])
        else:
            m = re.match(r'cloudflared_tunnel_response_by_code\{status_code="(\d+)"\} (\S+)', line)
            if m:
                codes[m.group(1)] = float(m.group(2))
    return {"requests": total, "errors": errors, "by_code": codes}


def machine():
    meminfo = {k: int(v.split()[0]) for k, v in (l.split(":", 1) for l in open("/proc/meminfo"))}
    disk = shutil.disk_usage(str(HOME))
    return {
        "load1": os.getloadavg()[0],
        "mem_available_mb": meminfo["MemAvailable"] // 1024,
        "disk_used_pct": round(disk.used * 100 / disk.total, 1),
        "uptime_s": int(float(open("/proc/uptime").read().split()[0])),
        "boot_id": open("/proc/sys/kernel/random/boot_id").read().strip(),
    }


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    now = datetime.now(timezone.utc)
    sample = {"ts": now.isoformat(timespec="seconds")}
    sample["public"] = {k: fetch(u) for k, u in PUBLIC.items()}
    sample["local"] = {k: fetch(u, 5) for k, u in LOCAL.items()}
    sample["units"] = {u: unit_active(u) for u in UNITS}
    sample["database"] = database_healthy()
    sample["tunnel"] = tunnel_counters()
    sample["machine"] = machine()
    with open(OUT / f"samples-{now:%Y%m%d}.jsonl", "a") as f:
        f.write(json.dumps(sample, separators=(",", ":")) + "\n")
    cutoff = time.time() - KEEP_DAYS * 86400
    for old in OUT.glob("samples-*.jsonl"):
        if old.stat().st_mtime < cutoff:
            old.unlink()


if __name__ == "__main__":
    sys.exit(main())
