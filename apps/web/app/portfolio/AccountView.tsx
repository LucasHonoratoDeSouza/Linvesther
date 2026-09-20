import { fetchBinanceSeries, type BinanceNav, type BinancePerformance } from "../../lib/api";
import { dateLabel, money, percent, priceDigits, quantity, signedMoney } from "./format";
import { PerformanceChart } from "./PerformanceChart";
import { usePrivacy } from "./privacy";
import styles from "./portfolio.module.css";

function Metric({
  label,
  hint,
  testId,
  value,
  missing,
  reason,
}: {
  label: string;
  hint: string;
  testId: string;
  value: string | null;
  missing: string;
  reason?: string | null;
}) {
  return (
    <div className={styles.metricTile} title={reason ?? undefined}>
      <div className={styles.metricTileLabel}>{label}</div>
      <div className={`${styles.metricTileValue}${value === null ? ` ${styles.unavailable}` : ""}`} data-testid={testId}>
        {value ?? missing}
      </div>
      <div className={styles.metricTileSub}>{hint}</div>
    </div>
  );
}

/** Everything for one connected account: profit since it was connected,
 * how it performed, and what it holds. */
export function AccountView({
  accountId,
  sinceMs,
  refreshKey,
  nav,
  performance,
  loadingPerformance,
}: {
  accountId: string;
  /** When this account was connected (epoch ms) — decides which chart ranges are unlocked. */
  sinceMs: number;
  /** Changes whenever the numbers are refreshed, so the chart reloads with them. */
  refreshKey: number;
  nav: BinanceNav | null;
  performance: BinancePerformance | null;
  loadingPerformance: boolean;
}) {
  const winRateLabel = performance?.winRate ? `${(Number(performance.winRate.winRate) * 100).toFixed(1)}%` : null;
  const winRateSub = performance?.winRate
    ? `${performance.winRate.wins}/${performance.winRate.closedRoundTrips} closed round trips (FIFO)`
    : "Share of closed trades that made money";
  const profit = performance ? Number(performance.pnl.realized) + Number(performance.pnl.unrealized) : 0;
  const { mask, tone } = usePrivacy();

  return (
    <div>
      {performance && (
        <div className={styles.card} data-testid="pnl-card">
          <div className={styles.cardTitleRow}>
            <div className={styles.cardTitle}>Profit since you connected</div>
            {performance.earliestReliableCheckpointMs != null && (
              <span className={styles.badge} data-testid="performance-since">
                Since {dateLabel(performance.earliestReliableCheckpointMs)}
              </span>
            )}
          </div>
          <div className={`${styles.pnlTotal} ${tone(profit)}`} data-testid="pnl-total">
            {mask(signedMoney(profit))} <sup>{nav?.currency ?? "USDT"}</sup>
          </div>
          <div className={styles.pnlBreakdown}>
            <div>
              <span>Realized</span>
              <strong className={tone(performance.pnl.realized)} data-testid="pnl-realized">
                {mask(signedMoney(performance.pnl.realized))}
              </strong>
              <small>from positions you closed</small>
            </div>
            <div>
              <span>Unrealized</span>
              <strong className={tone(performance.pnl.unrealized)} data-testid="pnl-unrealized">
                {mask(signedMoney(performance.pnl.unrealized))}
              </strong>
              <small>on what you still hold</small>
            </div>
            <div>
              <span>Fees paid</span>
              <strong data-testid="pnl-fees">{mask(money(performance.pnl.fees))}</strong>
              <small>not deducted above</small>
            </div>
          </div>
          <p className={styles.note} style={{ marginTop: 14, marginBottom: 0 }}>
            Only what happens after you connected is counted: what you already held then is the starting point, not a
            gain. Deposits and withdrawals never count as profit or loss.
          </p>
        </div>
      )}

      <div className={styles.card}>
        <div className={styles.cardTitleRow}>
          <div className={styles.cardTitle}>Performance</div>
        </div>
        {loadingPerformance && !performance && (
          <p className={styles.note} data-testid="performance-loading">
            <span className={styles.spinner} aria-hidden="true" /> Analysing your trading history…
          </p>
        )}
        {performance && (
          <div data-testid="performance-result">
            <PerformanceChart sinceMs={sinceMs} refreshKey={refreshKey} load={(range) => fetchBinanceSeries(accountId, range)} allowBalance />
            <div className={styles.metricGrid} style={{ marginTop: 16 }}>
              <Metric
                label="Return per year"
                hint="Annualised growth (CAGR)"
                testId="metric-cagr"
                value={performance.cagr ? percent(performance.cagr) : null}
                missing="Needs 1 year of history"
                reason={performance.cagrUnavailable}
              />
              <Metric
                label="Worst drop"
                hint="Biggest fall from a peak (max drawdown)"
                testId="metric-max-drawdown"
                value={performance.maxDrawdown ? percent(performance.maxDrawdown) : null}
                missing="Not enough history yet"
              />
              <Metric
                label="Return vs. risk"
                hint="Sharpe ratio — higher is steadier"
                testId="metric-sharpe"
                value={performance.sharpe ? Number(performance.sharpe).toFixed(2) : null}
                missing="Needs 30 days of history"
                reason={performance.sharpeUnavailable}
              />
              <Metric
                label="Winning trades"
                hint={winRateSub}
                testId="metric-win-rate"
                value={winRateLabel}
                missing="No closed trades yet"
              />
            </div>
            <p className={styles.note} style={{ marginTop: 14, marginBottom: 0 }}>
              Only activity Linvesther could verify from your exchange is counted, and every symbol is included
              automatically — you can’t pick which trades or period to show.
            </p>
          </div>
        )}
      </div>

      {nav && nav.assets.length > 0 && (
        <div className={styles.card}>
          <div className={styles.cardTitleRow}>
            <div className={styles.cardTitle}>What you hold</div>
          </div>
          {[...nav.assets]
            .sort((a, b) => Number(b.value) - Number(a.value))
            .map((asset) => {
              const share = Number(nav.nav) > 0 ? Number(asset.value) / Number(nav.nav) : 0;
              const position = performance?.pnl.positions.find((p) => p.asset === asset.asset);
              return (
                <div className={styles.assetRow} key={asset.asset}>
                  <div className={styles.assetInfo}>
                    <div className={styles.assetName}>{asset.asset}</div>
                    <div className={styles.assetQty}>
                      {mask(`${quantity(asset.quantity)} × ${money(asset.price, priceDigits(asset.price))}`)}
                      {position?.averageCost != null && (
                        <> · bought at avg {mask(money(position.averageCost, priceDigits(position.averageCost)))}</>
                      )}
                    </div>
                    <div className={styles.shareBar} aria-hidden="true">
                      <span style={{ width: `${Math.max(share * 100, 1)}%` }} />
                    </div>
                  </div>
                  <div className={styles.assetRight}>
                    <div className={styles.assetUsd}>
                      {mask(money(asset.value))} {nav.currency}
                    </div>
                    {position?.unrealized != null && (
                      <div className={`${styles.assetQty} ${tone(position.unrealized)}`}>
                        {mask(signedMoney(position.unrealized))} since you connected
                      </div>
                    )}
                    <div className={styles.assetQty}>{(share * 100).toFixed(1)}% of portfolio</div>
                  </div>
                </div>
              );
            })}
        </div>
      )}
    </div>
  );
}
