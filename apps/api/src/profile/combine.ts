import type { BinanceSeries } from "../binance-connect/types.js";

const DAY_MS = 86_400_000;

/** One account's value and return index over time. */
type AccountSeries = Pick<BinanceSeries, "points" | "sinceMs">;

/** Several accounts' return curves as one: over each interval, each
 * account counts in proportion to what it held at the start of it. An
 * account only starts counting from when it was connected. Absolute values
 * are used for weighting only — the result is returns, nothing else. */
export function combineReturns(accounts: AccountSeries[]): { timeMs: number; growth: number }[] {
  const times = [...new Set(accounts.flatMap((a) => a.points.map((p) => p.timeMs)))].sort((a, b) => a - b);
  const cursors = accounts.map(() => -1);
  const curve: { timeMs: number; growth: number }[] = [];
  let growth = 1;
  let previous: { nav: number; index: number }[] | null = null;

  for (const time of times) {
    const now = accounts.map((a, i) => {
      while (cursors[i]! + 1 < a.points.length && a.points[cursors[i]! + 1]!.timeMs <= time) cursors[i]!++;
      const point = a.points[cursors[i]!];
      return point ? { nav: Number(point.nav), index: Number(point.index) } : { nav: NaN, index: NaN };
    });
    if (previous) {
      let weighted = 0;
      let total = 0;
      now.forEach((current, i) => {
        const before = previous![i]!;
        if (Number.isNaN(before.nav) || Number.isNaN(current.index) || before.index === 0) return;
        weighted += before.nav * (current.index / before.index - 1);
        total += before.nav;
      });
      if (total > 0) growth *= 1 + weighted / total;
    }
    curve.push({ timeMs: time, growth });
    previous = now;
  }
  return curve;
}

export function maxDrawdown(curve: { growth: number }[]): number {
  let peak = 0;
  let worst = 0;
  for (const { growth } of curve) {
    peak = Math.max(peak, growth);
    if (peak > 0) worst = Math.max(worst, (peak - growth) / peak);
  }
  return worst;
}

/** Annualised Sharpe from the curve sampled once a day; null until there are 30 days. */
export function sharpe(curve: { timeMs: number; growth: number }[]): number | null {
  if (curve.length === 0) return null;
  const daily: number[] = [];
  let lastGrowth = curve[0]!.growth;
  let lastTime = curve[0]!.timeMs;
  for (const point of curve) {
    if (point.timeMs - lastTime >= DAY_MS) {
      daily.push(point.growth / lastGrowth - 1);
      lastGrowth = point.growth;
      lastTime = point.timeMs;
    }
  }
  if (daily.length < 30) return null;
  const mean = daily.reduce((a, b) => a + b, 0) / daily.length;
  const variance = daily.reduce((a, b) => a + (b - mean) ** 2, 0) / (daily.length - 1);
  return variance > 0 ? (mean / Math.sqrt(variance)) * Math.sqrt(365) : null;
}
