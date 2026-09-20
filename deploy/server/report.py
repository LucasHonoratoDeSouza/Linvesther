#!/usr/bin/env python3
"""Summarises the samples: availability, outages, reboots, latency and traffic.

Usage: report.py [--hours N]   (default 24)
"""
import argparse, json
from datetime import datetime, timedelta, timezone
from pathlib import Path

OUT = Path.home() / "linvesther" / "metrics"
GAP = timedelta(minutes=3)  # samples come every minute; a longer silence means the machine was off
CHECKS = ["site", "app", "docs", "api"]


def load(since):
    rows = []
    for path in sorted(OUT.glob("samples-*.jsonl")):
        for line in path.read_text().splitlines():
            try:
                row = json.loads(line)
            except ValueError:
                continue
            row["t"] = datetime.fromisoformat(row["ts"])
            if row["t"] >= since:
                rows.append(row)
    return sorted(rows, key=lambda r: r["t"])


def ok(row):
    return all(200 <= row["public"][c]["status"] < 400 for c in CHECKS)


def pct(values, q):
    values = sorted(values)
    return values[min(len(values) - 1, int(len(values) * q))] if values else 0


def fmt(t):
    return t.astimezone().strftime("%d/%m %H:%M")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--hours", type=float, default=24)
    args = parser.parse_args()
    now = datetime.now(timezone.utc)
    since = now - timedelta(hours=args.hours)
    rows = load(since)
    if not rows:
        print("no samples in this period")
        return
    print(f"Period: {fmt(since)} to {fmt(now)} ({args.hours:g} h), {len(rows)} samples\n")

    # Availability, counting only the time we were actually sampling.
    up = sum(1 for r in rows if ok(r))
    print(f"Availability while sampled: {100 * up / len(rows):.2f}% ({len(rows) - up} bad samples)")
    for c in CHECKS:
        good = sum(1 for r in rows if 200 <= r["public"][c]["status"] < 400)
        lat = [r["public"][c]["ms"] for r in rows if r["public"][c]["status"]]
        print(f"  {c:5} {100 * good / len(rows):6.2f}%  latency p50 {pct(lat, .5)} ms  p95 {pct(lat, .95)} ms")

    # Outages: runs of failing samples, and silences longer than a few minutes.
    events = []
    start = None
    prev = None
    for r in rows:
        if prev and r["t"] - prev["t"] > GAP:
            events.append((prev["t"], r["t"], "no samples (machine or network off)"))
        if not ok(r) and start is None:
            start = r["t"]
        if ok(r) and start is not None:
            events.append((start, r["t"], "public address failing"))
            start = None
        prev = r
    if start is not None:
        events.append((start, prev["t"], "public address failing (still)"))
    print("\nOutages:")
    if not events:
        print("  none")
    for a, b, why in events:
        print(f"  {fmt(a)} to {fmt(b)} ({int((b - a).total_seconds() // 60)} min): {why}")

    boots = [(p["t"], r["t"]) for p, r in zip(rows, rows[1:]) if p["machine"]["boot_id"] != r["machine"]["boot_id"]]
    print(f"\nReboots: {len(boots)}" + "".join(f"\n  {fmt(b)}" for _, b in boots))

    # Traffic: the tunnel's counters only grow, so use differences (a drop is a restart).
    per_hour = {}
    prev = None
    for r in rows:
        cur = r.get("tunnel")
        if cur and cur.get("requests") is not None and prev:
            delta = cur["requests"] - prev["requests"]
            if delta < 0:
                delta = cur["requests"]
            hour = r["t"].astimezone().strftime("%d/%m %H:00")
            per_hour[hour] = per_hour.get(hour, 0) + delta
        prev = cur if cur and cur.get("requests") is not None else prev
    print("\nTraffic (requests through the tunnel):")
    if per_hour:
        print(f"  total {int(sum(per_hour.values()))}")
        for hour, n in sorted(per_hour.items(), key=lambda kv: -kv[1])[:5]:
            print(f"  peak hour {hour}: {int(n)}")
    else:
        print("  no data yet")

    last = rows[-1]["machine"]
    print(f"\nMachine now: load {last['load1']:.2f}, {last['mem_available_mb']} MB free, disk {last['disk_used_pct']}%, up {last['uptime_s'] // 3600} h")
    print(f"Peak load {max(r['machine']['load1'] for r in rows):.2f}, lowest free memory {min(r['machine']['mem_available_mb'] for r in rows)} MB")


if __name__ == "__main__":
    main()
