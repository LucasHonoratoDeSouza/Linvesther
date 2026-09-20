"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import type { ApiResult, BinanceSeries, SeriesRange } from "../../lib/api";
import { money } from "./format";
import { usePrivacy } from "./privacy";
import styles from "./portfolio.module.css";

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

/** Longer ranges unlock as the account's history grows past their span — a
 * "1 year" chart of an account connected last week would be a lie. */
const RANGES: { key: SeriesRange; label: string; span: number; unlock: string }[] = [
  { key: "24h", label: "24H", span: DAY, unlock: "24 hours" },
  { key: "7d", label: "7D", span: 7 * DAY, unlock: "7 days" },
  { key: "30d", label: "30D", span: 30 * DAY, unlock: "30 days" },
  { key: "1y", label: "1Y", span: 365 * DAY, unlock: "1 year" },
  { key: "5y", label: "5Y", span: 5 * 365 * DAY, unlock: "5 years" },
  { key: "max", label: "Since connected", span: 0, unlock: "" },
];

const W = 640;
const H = 240;
const PAD = { left: 58, right: 14, top: 14, bottom: 26 };

function timeLabel(ms: number, spanMs: number): string {
  const d = new Date(ms);
  if (spanMs <= 2 * DAY) return d.toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit" });
  if (spanMs <= 400 * DAY) return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
  return d.toLocaleDateString("en-US", { month: "short", year: "numeric" });
}

function tooltipTime(ms: number): string {
  return new Date(ms).toLocaleString("en-US", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

const pct = (value: number) => {
  const digits = Math.abs(value) < 0.1 ? 3 : 2;
  return `${value > 0 ? "+" : ""}${value.toFixed(digits)}%`;
};

/** How the account has done since it was connected, drawn from real
 * market prices — honest about scale (never zoomed so far that a tiny move
 * looks like a big one) and about what it can show yet. */
export function PerformanceChart({
  sinceMs,
  refreshKey,
  load,
  allowBalance,
}: {
  /** When the account was connected (epoch ms) — decides which ranges are unlocked. */
  sinceMs: number;
  /** Changes whenever the numbers are refreshed, so the chart reloads with them. */
  refreshKey: number;
  /** Where the points come from: the owner's own series, or a public one (percentages only). */
  load: (range: SeriesRange) => Promise<ApiResult<BinanceSeries>>;
  /** The owner's view can also plot the account's value; a public view never can. */
  allowBalance: boolean;
}) {
  const { hidden, mask } = usePrivacy();
  const [range, setRange] = useState<SeriesRange>("max");
  const [chosenMode, setMode] = useState<"return" | "balance">("return");
  const mode = allowBalance ? chosenMode : "return";
  const [series, setSeries] = useState<BinanceSeries | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [hover, setHover] = useState<number | null>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const [now, setNow] = useState(() => Date.now());

  // Age advances while the page is open, so a range can unlock without a reload.
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(timer);
  }, []);
  const age = now - sinceMs;

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setFailure(null);
    load(range).then((result) => {
      if (cancelled) return;
      if (result.ok) setSeries(result.data);
      else setFailure(result.error);
      setLoading(false);
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [range, refreshKey]);

  const geometry = useMemo(() => {
    if (!series || series.points.length < 2) return null;
    const values = series.points.map((p) => (mode === "return" ? (Number(p.index) - 1) * 100 : Number(p.nav)));
    let lo = Math.min(...values);
    let hi = Math.max(...values);
    if (mode === "return") {
      // Always include the 0% line, and never zoom in past ±0.25%: a
      // hair's-breadth move must look like one.
      lo = Math.min(lo, 0);
      hi = Math.max(hi, 0);
      if (hi - lo < 0.5) {
        const mid = (hi + lo) / 2;
        lo = mid - 0.25;
        hi = mid + 0.25;
      }
    } else {
      const floor = Math.max(hi, 1) * 0.004;
      if (hi - lo < floor) {
        const mid = (hi + lo) / 2;
        lo = mid - floor / 2;
        hi = mid + floor / 2;
      }
    }
    const pad = (hi - lo) * 0.08;
    lo -= pad;
    hi += pad;
    const t0 = series.points[0]!.timeMs;
    const t1 = series.points[series.points.length - 1]!.timeMs;
    const x = (t: number) => PAD.left + ((t - t0) / Math.max(t1 - t0, 1)) * (W - PAD.left - PAD.right);
    const y = (v: number) => PAD.top + (1 - (v - lo) / (hi - lo)) * (H - PAD.top - PAD.bottom);
    const coords = series.points.map((p, i) => ({ x: x(p.timeMs), y: y(values[i]!), value: values[i]!, timeMs: p.timeMs }));
    const line = coords.map((c, i) => `${i === 0 ? "M" : "L"} ${c.x.toFixed(1)} ${c.y.toFixed(1)}`).join(" ");
    const area = `${line} L ${coords[coords.length - 1]!.x.toFixed(1)} ${H - PAD.bottom} L ${coords[0]!.x.toFixed(1)} ${H - PAD.bottom} Z`;
    const yTicks = [0, 1, 2, 3].map((i) => lo + ((hi - lo) * i) / 3);
    const xTicks = [0, 1, 2, 3].map((i) => t0 + ((t1 - t0) * i) / 3);
    return { coords, line, area, y, x, yTicks, xTicks, t0, t1, first: values[0]!, last: values[values.length - 1]! };
  }, [series, mode]);

  const spanMs = series && series.points.length > 1 ? series.points[series.points.length - 1]!.timeMs - series.points[0]!.timeMs : 0;
  const change = geometry ? geometry.last - geometry.first : 0;

  function onMove(event: React.PointerEvent<SVGSVGElement>) {
    if (!geometry || !svgRef.current) return;
    const rect = svgRef.current.getBoundingClientRect();
    const px = ((event.clientX - rect.left) / rect.width) * W;
    let best = 0;
    for (let i = 1; i < geometry.coords.length; i++) {
      if (Math.abs(geometry.coords[i]!.x - px) < Math.abs(geometry.coords[best]!.x - px)) best = i;
    }
    setHover(best);
  }

  const hovered = geometry && hover !== null ? geometry.coords[hover] : null;
  const yLabel = (v: number) => (mode === "return" ? pct(v) : mask(money(String(v), v < 100 ? 2 : 0)));

  return (
    <div data-testid="performance-chart">
      <div className={styles.chartBar}>
        <div className={styles.rangeTabs} role="tablist" aria-label="Time range">
          {RANGES.map((option) => {
            const locked = option.key !== "max" && age < option.span;
            return (
              <button
                key={option.key}
                type="button"
                role="tab"
                aria-selected={range === option.key}
                disabled={locked}
                title={locked ? `Unlocks once the account has ${option.unlock} of history` : undefined}
                data-testid={`range-${option.key}`}
                className={`${styles.rangeTab} ${range === option.key ? styles.rangeTabOn : ""}`}
                onClick={() => setRange(option.key)}
              >
                {option.label}
              </button>
            );
          })}
        </div>
        {allowBalance && (
        <div className={styles.rangeTabs} role="tablist" aria-label="What to plot">
          {(["return", "balance"] as const).map((m) => (
            <button
              key={m}
              type="button"
              role="tab"
              aria-selected={mode === m}
              data-testid={`mode-${m}`}
              className={`${styles.rangeTab} ${mode === m ? styles.rangeTabOn : ""}`}
              onClick={() => setMode(m)}
            >
              {m === "return" ? "Return" : "Balance"}
            </button>
          ))}
        </div>
        )}
      </div>

      {failure && (
        <p className={styles.note} role="alert">
          Couldn’t load the chart: {failure}
        </p>
      )}

      {!geometry && !failure && (
        <p className={styles.note} data-testid="chart-collecting" style={{ textAlign: "center", padding: "40px 0" }}>
          {loading ? "Loading the chart…" : "Collecting data — the chart fills in as time passes."}
        </p>
      )}

      {geometry && (
        <div style={{ opacity: loading ? 0.55 : 1, transition: "opacity 120ms" }}>
          <div className={styles.chartSummary} data-testid="chart-summary">
            <strong className={hidden && mode === "balance" ? "" : change > 0 ? styles.pnlUp : change < 0 ? styles.pnlDown : ""}>
              {mode === "return" ? pct(change) : mask(`${change > 0 ? "+" : ""}${money(String(change))}`)}
            </strong>{" "}
            <span>
              {mode === "return" ? "return" : "change in balance (includes deposits and withdrawals)"} over{" "}
              {range === "max" ? "the time since you connected" : RANGES.find((r) => r.key === range)?.label}
            </span>
          </div>
          <svg
            ref={svgRef}
            viewBox={`0 0 ${W} ${H}`}
            role="img"
            aria-label={mode === "return" ? "Return over time" : "Balance over time"}
            className={styles.chartSvg}
            onPointerMove={onMove}
            onPointerLeave={() => setHover(null)}
            data-testid="chart-svg"
          >
            <defs>
              <linearGradient id="chartLine" x1="0" y1="0" x2="1" y2="0">
                <stop offset="0%" stopColor="#0f7a4a" />
                <stop offset="100%" stopColor="#97fce3" />
              </linearGradient>
              <linearGradient id="chartFill" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="#0f7a4a" stopOpacity="0.2" />
                <stop offset="100%" stopColor="#0f7a4a" stopOpacity="0" />
              </linearGradient>
            </defs>
            {geometry.yTicks.map((tick) => (
              <g key={tick}>
                <line x1={PAD.left} x2={W - PAD.right} y1={geometry.y(tick)} y2={geometry.y(tick)} stroke="#2c302a" strokeWidth={1} />
                <text x={PAD.left - 8} y={geometry.y(tick) + 3.5} textAnchor="end" className={styles.axisText}>
                  {yLabel(tick)}
                </text>
              </g>
            ))}
            {mode === "return" && (
              <line x1={PAD.left} x2={W - PAD.right} y1={geometry.y(0)} y2={geometry.y(0)} stroke="#6f7c63" strokeWidth={1} strokeDasharray="4 4" />
            )}
            {geometry.xTicks.map((tick, i) => (
              <text
                key={tick}
                x={geometry.x(tick)}
                y={H - 7}
                textAnchor={i === 0 ? "start" : i === geometry.xTicks.length - 1 ? "end" : "middle"}
                className={styles.axisText}
              >
                {timeLabel(tick, spanMs)}
              </text>
            ))}
            <path d={geometry.area} fill="url(#chartFill)" />
            <path d={geometry.line} fill="none" stroke="url(#chartLine)" strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />
            {hovered && (
              <g pointerEvents="none">
                <line x1={hovered.x} x2={hovered.x} y1={PAD.top} y2={H - PAD.bottom} stroke="#536643" strokeWidth={1} />
                <circle cx={hovered.x} cy={hovered.y} r={4} fill="#d0edaa" stroke="#131513" strokeWidth={2} />
              </g>
            )}
          </svg>
          <div className={styles.chartHover} data-testid="chart-hover" aria-live="off">
            {hovered ? (
              <>
                <span>{tooltipTime(hovered.timeMs)}</span> <strong>{yLabel(hovered.value)}</strong>
              </>
            ) : (
              <span className={styles.note}>Move over the chart to read a point.</span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
