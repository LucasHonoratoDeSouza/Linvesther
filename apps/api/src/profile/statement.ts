import type { BinancePerformance } from "../binance-connect/types.js";
import type { ProfileStatement, ShareableMetric } from "./types.js";

const DAY_MS = 86_400_000;

/** The compounded return over every per-period return of the series. */
function compoundedReturn(dailyReturns: string[]): string | null {
  if (dailyReturns.length === 0) return null;
  const growth = dailyReturns.reduce((acc, r) => acc * (1 + Number(r)), 1);
  return (growth - 1).toFixed(6);
}

export interface BuiltStatement {
  statement: ProfileStatement;
  /** Metrics the person asked to share that can't be computed yet
   * (not enough history) — left out, never estimated. */
  unavailable: ShareableMetric[];
}

/** Builds what an account shows publicly from the *server's own*
 * computation — no client ever supplies a number. Metrics the owner hid
 * are absent, and any that can't be computed yet are reported in
 * `unavailable` rather than shown as a guess. */
export function buildStatement(input: {
  address: `0x${string}`;
  accountId: string;
  trackName: string;
  share: ShareableMetric[];
  performance: Pick<BinancePerformance, "dailyReturns" | "maxDrawdown" | "sharpe" | "winRate" | "earliestReliableCheckpointMs">;
  now: Date;
}): BuiltStatement {
  const { performance, share, now } = input;
  const metrics: ProfileStatement["metrics"] = {};
  const unavailable: ShareableMetric[] = [];

  for (const metric of new Set(share)) {
    if (metric === "return") {
      const value = compoundedReturn(performance.dailyReturns);
      if (value === null) unavailable.push(metric);
      else metrics.return = value;
    } else if (metric === "maxDrawdown") {
      if (performance.maxDrawdown === null) unavailable.push(metric);
      else metrics.maxDrawdown = performance.maxDrawdown;
    } else if (metric === "sharpe") {
      if (performance.sharpe === null) unavailable.push(metric);
      else metrics.sharpe = performance.sharpe;
    } else if (metric === "winRate") {
      if (performance.winRate === null) unavailable.push(metric);
      else metrics.winRate = { value: performance.winRate.winRate, closedTrades: performance.winRate.closedRoundTrips };
    }
  }

  const sinceMs = performance.earliestReliableCheckpointMs ?? now.getTime();
  return {
    statement: {
      address: input.address,
      accountId: input.accountId,
      trackName: input.trackName,
      since: new Date(sinceMs).toISOString(),
      trackedDays: Math.max(0, Math.floor((now.getTime() - sinceMs) / DAY_MS)),
      verification: "collector_attested_read_only",
      metrics,
      computedAt: now.toISOString(),
    },
    unavailable,
  };
}
