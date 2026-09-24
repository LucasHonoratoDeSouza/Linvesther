// The six tools this server offers, and nothing else.
//
// Every description says plainly that the tool reads and cannot act, and
// every tool is annotated `readOnlyHint` so a client can see the same
// thing without reading prose. There is no tool that connects an
// account, places or cancels an order, moves funds, or changes any
// setting — not disabled, not gated: absent, and unreachable from the
// code this server imports.
//
// Input and output schemas are declared for each tool, so `tools/list`
// publishes a JSON Schema for both and every answer is validated against
// its own shape before it leaves.

import { z } from "zod";
import type { ReadOnlyAccountApi } from "./api.js";

/** Which account to read. Left out, the answer is the account whose id
 * is the token owner's own address — the one every identity starts
 * with. */
const accountId = z
  .string()
  .max(80)
  .optional()
  .describe("Which connected account to read. Omit for the token owner's own account.");

/** Decimal figures cross as strings, never numbers: a double cannot hold
 * every value a balance can take. */
const decimal = z.string();

const RANGES = ["24h", "7d", "30d", "1y", "5y", "max"] as const;

const connectionShape = z
  .object({
    connected: z.boolean(),
    status: z.string().nullable(),
    lastSyncedAt: z.string().nullable(),
    tradeCount: z.number(),
    flowCount: z.number(),
  })
  .nullable();

const stateOutput = {
  address: z.string().describe("The identity the token belongs to."),
  scope: z.string().describe("What the token may do. Always read:account."),
  accounts: z.array(
    z.object({
      accountId: z.string(),
      broker: z.string().describe("binance, coinbase, kraken, ibkr or wallet."),
      label: z.string().nullable(),
      connectedAtMs: z.number().describe("Every figure for this account is measured from here."),
      status: z.string(),
      connection: connectionShape,
      connectionUnavailable: z.string().nullable(),
      balance: z.object({ nav: decimal, currency: z.string() }).nullable(),
    }),
  ),
  balance: z
    .object({ total: decimal, currency: z.string() })
    .nullable()
    .describe("Every account added together. Null when it cannot be stated exactly."),
  balanceUnavailable: z.string().nullable().describe("Why there is no total, when there is none."),
};

const portfolioOutput = {
  accountId: z.string(),
  positions: z.array(
    z.object({
      asset: z.string(),
      quantity: decimal,
      averageCost: decimal.nullable(),
      markPrice: decimal,
      realized: decimal,
      unrealized: decimal.nullable(),
    }),
  ),
  realized: decimal,
  unrealized: decimal,
  fees: decimal,
  sinceMs: z.number().nullable().describe("When the account was connected; profit is measured from then."),
};

const navOutput = {
  accountId: z.string(),
  nav: decimal.describe("What the account is worth right now."),
  currency: z.string(),
  assets: z.array(z.object({ asset: z.string(), quantity: decimal, price: decimal, value: decimal })),
  excludedOutOfScopeAssets: z.array(z.string()).describe("Held assets this valuation does not cover."),
};

const performanceOutput = {
  accountId: z.string(),
  cagr: decimal.nullable(),
  cagrUnavailable: z.string().nullable(),
  sharpe: decimal.nullable(),
  sharpeUnavailable: z.string().nullable(),
  sortino: decimal.nullable(),
  sortinoUnavailable: z.string().nullable(),
  maxDrawdown: decimal.nullable(),
  winRate: z.object({ closedRoundTrips: z.number(), wins: z.number(), winRate: decimal }).nullable(),
  sinceMs: z.number().nullable(),
};

const tradesOutput = {
  accountId: z.string(),
  trades: z.array(
    z.object({
      symbol: z.string(),
      tradeId: z.number(),
      orderId: z.number(),
      price: decimal,
      quantity: decimal,
      commission: decimal,
      commissionAsset: z.string(),
      timeMs: z.number(),
      side: z.string().describe("buy or sell."),
    }),
  ),
  nextCursor: z.string().nullable().describe("Pass back as cursor for the next page. Null on the last one."),
  sinceMs: z.number(),
};

const seriesOutput = {
  accountId: z.string(),
  range: z.string(),
  points: z.array(
    z.object({
      timeMs: z.number(),
      nav: decimal.describe("Account value then; deposits and withdrawals move it."),
      index: decimal.describe("Time-weighted return index, 1 at the first point; flows never move it."),
    }),
  ),
  stepMs: z.number(),
  sinceMs: z.number(),
};

/** A tool as this server registers it. Kept as data so the set can be
 * counted, named and checked in a test, rather than being implied by a
 * sequence of registration calls. */
export interface ReadOnlyTool {
  name: string;
  title: string;
  description: string;
  inputSchema: z.ZodRawShape;
  outputSchema: z.ZodRawShape;
  /** What to ask the API. `input` has already been validated. */
  call: (api: ReadOnlyAccountApi, token: string, input: Record<string, never>) => Promise<unknown>;
}

const READS_ONLY = "Reads only: it cannot trade, transfer, connect or change anything.";

export const READ_ONLY_TOOLS: ReadOnlyTool[] = [
  {
    name: "get_account_state",
    title: "Account state",
    description: `Every exchange, broker and wallet account connected under this identity, how each connection is doing, what each is worth now, and their total. Credentials are never included. ${READS_ONLY}`,
    inputSchema: {},
    outputSchema: stateOutput,
    call: (api, token) => api.accountState(token),
  },
  {
    name: "get_portfolio",
    title: "Portfolio and profit",
    description: `Positions held, their average cost, and profit since the account was connected — realized, unrealized and fees. ${READS_ONLY}`,
    inputSchema: { accountId },
    outputSchema: portfolioOutput,
    call: (api, token, input) => api.portfolio(token, input),
  },
  {
    name: "get_nav",
    title: "Current value",
    description: `What the account is worth right now, from its real balance and real market prices, and which assets make it up. ${READS_ONLY}`,
    inputSchema: { accountId },
    outputSchema: navOutput,
    call: (api, token, input) => api.nav(token, input),
  },
  {
    name: "get_performance",
    title: "Performance metrics",
    description: `CAGR, Sharpe, Sortino, maximum drawdown and win rate. A metric with too little history comes back null with its own reason — never a zero standing in for one. ${READS_ONLY}`,
    inputSchema: { accountId },
    outputSchema: performanceOutput,
    call: (api, token, input) => api.performance(token, input),
  },
  {
    name: "get_trades",
    title: "Executed trades",
    description: `The trades already collected for this account, newest page last, filtered by market and period. Executions that happened, not orders that could be placed. ${READS_ONLY}`,
    inputSchema: {
      accountId,
      symbol: z.string().max(40).optional().describe("One market, e.g. BTCUSDT. Omit for every market."),
      since: z.number().int().nonnegative().optional().describe("Earliest execution time, epoch milliseconds, inclusive."),
      until: z.number().int().nonnegative().optional().describe("Latest execution time, epoch milliseconds, inclusive."),
      cursor: z.string().max(120).optional().describe("The nextCursor of the previous page."),
      limit: z.number().int().min(1).max(500).optional().describe("How many trades this page may carry. Default 100."),
    },
    outputSchema: tradesOutput,
    call: (api, token, input) => api.trades(token, input),
  },
  {
    name: "get_chart_series",
    title: "Value and return over time",
    description: `The account's value and time-weighted return over a range, as points for a chart. Deposits and withdrawals move the value and never the return index. ${READS_ONLY}`,
    inputSchema: {
      accountId,
      range: z.enum(RANGES).optional().describe("How far back to go. Default max, which starts at the connection."),
    },
    outputSchema: seriesOutput,
    call: (api, token, input) => api.series(token, input),
  },
];
