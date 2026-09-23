export interface BinanceConnectRequest {
  apiKey: string;
  apiSecret: string;
  /** Optional: symbols to backfill trade *history* for at connect
   * time. Ongoing trades on any symbol are captured in real time
   * regardless — see routes.ts. */
  symbols?: string[];
  /** Optional name the person gives this connected account ("Main"). */
  label?: string;
}

/** One exchange account connected under an identity. */
export interface ConnectedAccount {
  accountId: string;
  /** Which broker/exchange this account is at: `"binance"`, `"coinbase"`, `"kraken"` or `"ibkr"`. */
  broker: string;
  label: string | null;
  /** Epoch ms of the connection — every figure for this account is measured from then. */
  connectedAtMs: number;
  status: string;
}

export interface CoinbaseConnectRequest {
  keyName: string;
  privateKey: string;
  label?: string;
}

export interface KrakenConnectRequest {
  apiKey: string;
  apiSecret: string;
  label?: string;
}

export interface IbkrConnectRequest {
  token: string;
  queryId: string;
  label?: string;
}

export interface BinanceConnectResult {
  connectionId: string;
  summary: {
    tradesFetched: number;
    flowsFetched: number;
    catalogSymbolsFetched: number;
  };
}

export interface BinanceConnectionStatus {
  connected: boolean;
  status: string | null;
  last_synced_at: string | null;
  trade_count: number;
  flow_count: number;
}

export interface BinanceNavAsset {
  asset: string;
  quantity: string;
  price: string;
  value: string;
}

/** A real, current NAV snapshot — not a historical series. See
 * `services/collector/binance/worker/src/publish.rs` for exactly what
 * this does and does not compute. */
export interface BinanceNav {
  nav: string;
  currency: string;
  assets: BinanceNavAsset[];
  excludedOutOfScopeAssets: string[];
}

export interface BinanceDailyNav {
  dayStartMs: number;
  nav: string;
}

export interface BinanceWinRate {
  closedRoundTrips: number;
  wins: number;
  winRate: string;
}

/** A real, historical performance summary — daily NAV/returns for a
 * chart, plus Sharpe/Sortino/MDD/CAGR/win-rate. Each metric is
 * independently `null`, paired with a `*Unavailable` reason, when its
 * own minimum sample isn't met yet — never a fabricated number. See
 * `services/collector/binance/worker/src/history.rs`. */
/** Portable, independently-verifiable proof from
 * `binance-worker prove-performance` (see that crate's `prove.rs`):
 * real RISC Zero proving over the account's real, flow-adjusted TWR
 * index — never the raw trades, flows, balance or account identifier,
 * which stay behind the envelope's commitments. `receiptHex`/
 * `journalHex` are the same `bincode`+hex format
 * `crates/verifier::receipt::verify_receipt` and `linvesther-verify`
 * already expect. */
export interface BinancePerformanceProof {
  envelopeDigestHex: string;
  periodStartMs: number;
  periodEndMs: number;
  returnFraction: string;
  maxDrawdownBp: number;
  imageIdHex: string;
  receiptHex: string;
  journalHex: string;
}

export interface BinancePerformance {
  dailyNav: BinanceDailyNav[];
  dailyReturns: string[];
  sharpe: string | null;
  sharpeUnavailable: string | null;
  sortino: string | null;
  sortinoUnavailable: string | null;
  maxDrawdown: string | null;
  cagr: string | null;
  cagrUnavailable: string | null;
  winRate: BinanceWinRate | null;
  /** The instant the account was connected — every figure is measured from then on. */
  earliestReliableCheckpointMs: number | null;
  /** Profit since the connection, in USDT (average-cost basis; what was held at connection starts flat). */
  pnl: {
    positions: {
      asset: string;
      quantity: string;
      averageCost: string | null;
      markPrice: string;
      realized: string;
      unrealized: string | null;
    }[];
    realized: string;
    unrealized: string;
    fees: string;
  };
}

export const SERIES_RANGES = ["24h", "7d", "30d", "1y", "5y", "max"] as const;
export type SeriesRange = (typeof SERIES_RANGES)[number];

/** The account's value and return over a range, drawn from real market candles. */
export interface BinanceSeries {
  points: {
    timeMs: number;
    /** Account value in USDT then (deposits/withdrawals move it). */
    nav: string;
    /** Time-weighted return index, 1 at the first point (deposits/withdrawals never move it). */
    index: string;
  }[];
  stepMs: number;
  /** When the account was connected — no range reaches back before it. */
  sinceMs: number;
}
