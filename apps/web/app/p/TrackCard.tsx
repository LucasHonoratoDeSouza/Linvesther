"use client";

import type { ApiResult, BinanceSeries, PublicTrack, SeriesRange } from "../../lib/api";
import { fetchPublicSeries } from "../../lib/api";
import { PerformanceChart } from "../portfolio/PerformanceChart";
import styles from "./profile.module.css";

const pct = (fraction: string, digits = 2) => `${(Number(fraction) * 100).toFixed(digits)}%`;
const signedPct = (fraction: string) => `${Number(fraction) > 0 ? "+" : ""}${pct(fraction)}`;
const day = (iso: string) => new Date(iso).toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric" });

const METRIC_NAMES = { return: "Return", maxDrawdown: "Worst drop", sharpe: "Return vs. risk", winRate: "Winning trades" } as const;

/** One public track: only ratios, plainly where they come from and since
 * when. With `chart`, the return curve is drawn too — as percentages. */
export function TrackCard({ track, chart }: { track: PublicTrack; chart?: { address: string } }) {
  const { statement, unavailable } = track;
  const { metrics } = statement;
  const hasAny = Object.keys(metrics).length > 0;

  // The same chart the owner sees, fed only percentages: every point's
  // "index" is 1 + the return, and there is no balance to plot.
  const load = async (range: SeriesRange): Promise<ApiResult<BinanceSeries>> => {
    const result = await fetchPublicSeries(chart!.address, range);
    if (!result.ok) return result;
    return {
      ok: true,
      data: {
        points: result.data.points.map((p) => ({ timeMs: p.timeMs, nav: "0", index: String(1 + Number(p.returnFraction)) })),
        stepMs: result.data.stepMs,
        sinceMs: result.data.sinceMs,
      },
    };
  };

  return (
    <section className={styles.trackCard} data-testid="track-card">
      <div className={styles.trackHead}>
        <div>
          <h2 className={styles.trackName}>{statement.trackName}</h2>
          <div className={styles.trackMeta}>
            Tracked for {statement.trackedDays} {statement.trackedDays === 1 ? "day" : "days"} · since {day(statement.since)}
          </div>
        </div>
        <span className={styles.badge}>Read from the exchange, read-only</span>
      </div>

      {hasAny && (
        <div className={styles.tiles}>
          {metrics.return !== undefined && (
            <div className={styles.tile}>
              <div className={styles.tileLabel}>Return since tracking began</div>
              <div className={`${styles.tileValue} ${Number(metrics.return) >= 0 ? styles.up : styles.down}`} data-testid="track-return">
                {signedPct(metrics.return)}
              </div>
            </div>
          )}
          {metrics.maxDrawdown !== undefined && (
            <div className={styles.tile}>
              <div className={styles.tileLabel}>Worst drop from a peak</div>
              <div className={styles.tileValue} data-testid="track-drawdown">
                {pct(metrics.maxDrawdown)}
              </div>
            </div>
          )}
          {metrics.sharpe !== undefined && (
            <div className={styles.tile}>
              <div className={styles.tileLabel}>Return vs. risk (Sharpe)</div>
              <div className={styles.tileValue} data-testid="track-sharpe">
                {Number(metrics.sharpe).toFixed(2)}
              </div>
            </div>
          )}
          {metrics.winRate !== undefined && (
            <div className={styles.tile}>
              <div className={styles.tileLabel}>Winning trades</div>
              <div className={styles.tileValue} data-testid="track-win-rate">
                {pct(metrics.winRate.value, 1)}
              </div>
              <div className={styles.tileSub}>of {metrics.winRate.closedTrades} closed</div>
            </div>
          )}
        </div>
      )}
      {!hasAny && <p className={styles.trackFoot}>Not enough history yet — numbers appear here as the account builds a track record.</p>}
      {hasAny && unavailable.length > 0 && (
        <p className={styles.trackFoot} data-testid="track-unavailable">
          Still building history: {unavailable.map((key) => METRIC_NAMES[key]).join(", ")}.
        </p>
      )}

      {chart && statement.metrics.return !== undefined && (
        <div style={{ marginTop: 20 }}>
          <PerformanceChart sinceMs={Date.parse(statement.since)} refreshKey={0} load={load} allowBalance={false} />
        </div>
      )}

      <p className={styles.trackFoot}>
        Computed {day(statement.computedAt)} from the account’s read-only exchange data, and counted from when the
        account was connected — nothing earlier. Only percentages are shown: never balances, positions or trades.
      </p>
    </section>
  );
}
