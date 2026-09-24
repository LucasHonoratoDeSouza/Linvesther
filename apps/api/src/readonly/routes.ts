// The read-only account API: what an outside agent may read about the
// account whose token it holds.
//
// Every route here is a GET, authenticated only by a read-only token
// (`requireReadOnlyToken`), scoped to the identity that token belongs to
// (`isOwner`), and answered by the same worker reads the browser's own
// pages already use — no calculation is repeated here, only the layer
// that decides who is asking.
//
// This module imports `binance-connect/worker.ts` (the subprocess
// contract) and `binance-connect/types.ts` (types), never
// `binance-connect/routes.ts`: nothing reachable from here can connect,
// rename, remove or sync an account, or start a proof.

import type { FastifyInstance, FastifyReply, FastifyRequest } from "fastify";
import { isOwner } from "../auth/accountOwnership.js";
import { requireReadOnlyToken, type ReadOnlyPrincipal } from "../auth/requireReadOnlyToken.js";
import type { ReadOnlyTokenStore } from "../auth/readOnlyTokenStore.js";
import { boundedString } from "../security/input.js";
import { invokeWorker, WorkerInvocationError } from "../binance-connect/worker.js";
import {
  SERIES_RANGES,
  type BinanceConnectionStatus,
  type BinanceNav,
  type BinancePerformance,
  type BinanceSeries,
  type ConnectedAccount,
  type ExecutedTradePage,
  type SeriesRange,
} from "../binance-connect/types.js";
import { sumDecimals } from "./totals.js";

export interface ReadOnlyRouteOptions {
  tokenStore: ReadOnlyTokenStore;
  /** accountId -> the only address allowed to read it. The same map the
   * connection routes use. */
  accountOwners: Map<string, `0x${string}`>;
  /** Path to the compiled `binance-worker` binary. */
  workerBinaryPath: string;
  now?: () => Date;
}

/** Where every route here lives. Chosen so a deployment can put the
 * read-only API behind its own hostname, limits or firewall rule without
 * matching a list of paths. */
export const READ_ONLY_PREFIX = "/mcp";

/** Longest accountId worth looking at: an address (42) plus `_` plus the
 * 32-character name an extra account may carry. */
const MAX_ACCOUNT_ID = 80;

/** The most executions one page may carry — the worker refuses more. */
const MAX_TRADE_PAGE = 500;

interface Query {
  accountId?: string;
  range?: string;
  symbol?: string;
  since?: string;
  until?: string;
  cursor?: string;
  limit?: string;
}

type ReadOnlyRequest = FastifyRequest<{ Querystring: Query }>;

/** An epoch-millisecond bound from the query string. `undefined` when
 * absent, `null` when present but not a whole, non-negative number — the
 * three are different answers, and a bad one is a 400, never a silently
 * dropped filter. */
function epochMs(value: string | undefined): number | undefined | null {
  if (value === undefined) return undefined;
  if (!/^\d{1,15}$/.test(value)) return null;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) ? parsed : null;
}

function pageSize(value: string | undefined): number | undefined | null {
  if (value === undefined) return undefined;
  if (!/^\d{1,4}$/.test(value)) return null;
  const parsed = Number(value);
  return parsed >= 1 && parsed <= MAX_TRADE_PAGE ? parsed : null;
}

/** A refused token says which of the three it was, since the difference
 * matters to whoever configured the agent — but never whether some other
 * token exists. */
const REFUSALS = {
  unknown: { code: 401, error: "unauthenticated" },
  revoked: { code: 401, error: "token_revoked" },
  expired: { code: 401, error: "token_expired" },
} as const;

/** A failure the worker reported, answered the way the browser's own
 * routes answer it: 422 with the worker's own message, which never
 * carries a credential. Anything else is not ours to interpret. */
function workerFailure(reply: FastifyReply, error: unknown, code: string): FastifyReply {
  if (error instanceof WorkerInvocationError) {
    return reply.code(422).send({ error: code, detail: error.message });
  }
  throw error;
}

export function registerReadOnlyRoutes(app: FastifyInstance, options: ReadOnlyRouteOptions): void {
  const worker = { binaryPath: options.workerBinaryPath };
  const clock = options.now ?? (() => new Date());

  /** The identity behind the request's token, or `null` once the reply
   * has been sent. */
  async function caller(request: ReadOnlyRequest, reply: FastifyReply): Promise<ReadOnlyPrincipal | null> {
    const result = await requireReadOnlyToken(request, options.tokenStore, clock());
    if (typeof result === "string") {
      const { code, error } = REFUSALS[result];
      reply.code(code).send({ error });
      return null;
    }
    return result;
  }

  /** The caller and the account they asked about, in one step, so no
   * route here can read an account without both checks having run.
   *
   * With no `accountId` the answer is the account whose id is the token
   * owner's own address — the first one every identity gets (see
   * `accountOwnership.ts`), so the common case needs no parameter. */
  async function callerAndAccount(
    request: ReadOnlyRequest,
    reply: FastifyReply,
  ): Promise<{ principal: ReadOnlyPrincipal; accountId: string } | null> {
    const principal = await caller(request, reply);
    if (!principal) return null;
    const asked = request.query.accountId;
    if (asked !== undefined && boundedString(asked, MAX_ACCOUNT_ID) === undefined) {
      reply.code(400).send({ error: "accountId_invalid" });
      return null;
    }
    const accountId = asked === undefined ? principal.address : asked.trim();
    if (!isOwner(options.accountOwners, accountId, principal.address)) {
      reply.code(403).send({ error: "cross_account_access_denied" });
      return null;
    }
    return { principal, accountId };
  }

  // Which accounts this identity has connected, how each connection is
  // doing, and what each one is currently worth.
  //
  // The accounts and their health come from `list`/`status`; the current
  // value comes from `nav`, the same computation
  // `/accounts/:accountId/binance-nav` answers with. `nav` reaches the
  // exchange for live prices, so this is the most expensive read here,
  // and each account is read in turn rather than all at once — several
  // accounts at once would otherwise exhaust the worker's own
  // concurrency limit and come back as "busy".
  app.get<{ Querystring: Query }>(`${READ_ONLY_PREFIX}/account/state`, async (request, reply) => {
    const principal = await caller(request, reply);
    if (!principal) return reply;
    let connected: ConnectedAccount[];
    try {
      ({ accounts: connected } = await invokeWorker<{ accounts: ConnectedAccount[] }>(
        worker,
        "list",
        JSON.stringify({ ownerAddress: principal.address }),
      ));
    } catch (error) {
      return workerFailure(reply, error, "accounts_unavailable");
    }

    const accounts = [];
    const navs: string[] = [];
    const currencies = new Set<string>();
    let balanceUnavailable: string | null = connected.length === 0 ? "no account is connected" : null;
    for (const account of connected) {
      const payload = JSON.stringify({ accountId: account.accountId });
      let connection: BinanceConnectionStatus | null = null;
      let connectionUnavailable: string | null = null;
      try {
        connection = await invokeWorker<BinanceConnectionStatus>(worker, "status", payload);
      } catch (error) {
        if (!(error instanceof WorkerInvocationError)) throw error;
        connectionUnavailable = error.message;
      }
      let balance: { nav: string; currency: string } | null = null;
      try {
        const nav = await invokeWorker<BinanceNav>(worker, "nav", payload);
        balance = { nav: nav.nav, currency: nav.currency };
        navs.push(nav.nav);
        currencies.add(nav.currency);
      } catch (error) {
        if (!(error instanceof WorkerInvocationError)) throw error;
        // A total that silently left an account out would read as a real
        // figure while being wrong, so the whole total goes unavailable
        // with the reason instead of coming back smaller.
        balanceUnavailable ??= `${account.accountId}: ${error.message}`;
      }
      accounts.push({ ...account, connection, connectionUnavailable, balance });
    }

    if (balanceUnavailable === null && currencies.size > 1) {
      balanceUnavailable = `accounts are valued in more than one currency (${[...currencies].sort().join(", ")})`;
    }
    const total = balanceUnavailable === null ? sumDecimals(navs) : null;
    const currency = [...currencies][0];
    return {
      address: principal.address,
      scope: principal.scope,
      accounts,
      balance: total === null || currency === undefined ? null : { total, currency },
      balanceUnavailable: balanceUnavailable ?? (total === null ? "a balance was not a plain decimal figure" : null),
    };
  });

  // Positions, their average cost and the profit on them, realized and
  // not — `pnl.rs`'s computation, as `binance-performance` already
  // returns it.
  app.get<{ Querystring: Query }>(`${READ_ONLY_PREFIX}/account/portfolio`, async (request, reply) => {
    const found = await callerAndAccount(request, reply);
    if (!found) return reply;
    try {
      const performance = await invokeWorker<BinancePerformance>(worker, "performance", JSON.stringify({ accountId: found.accountId }));
      return {
        accountId: found.accountId,
        positions: performance.pnl.positions,
        realized: performance.pnl.realized,
        unrealized: performance.pnl.unrealized,
        fees: performance.pnl.fees,
        sinceMs: performance.earliestReliableCheckpointMs,
      };
    } catch (error) {
      return workerFailure(reply, error, "portfolio_unavailable");
    }
  });

  // What the account is worth right now, from its real balance and real
  // market prices.
  app.get<{ Querystring: Query }>(`${READ_ONLY_PREFIX}/account/nav`, async (request, reply) => {
    const found = await callerAndAccount(request, reply);
    if (!found) return reply;
    try {
      const nav = await invokeWorker<BinanceNav>(worker, "nav", JSON.stringify({ accountId: found.accountId }));
      return { accountId: found.accountId, ...nav };
    } catch (error) {
      return workerFailure(reply, error, "nav_unavailable");
    }
  });

  // CAGR, Sharpe, Sortino, max drawdown and win rate. Each is
  // independently `null` with its own reason when its minimum sample
  // isn't met — never a fabricated number. The daily arrays behind them
  // are not repeated here; `/account/series` is the read for a chart.
  app.get<{ Querystring: Query }>(`${READ_ONLY_PREFIX}/account/performance`, async (request, reply) => {
    const found = await callerAndAccount(request, reply);
    if (!found) return reply;
    try {
      const performance = await invokeWorker<BinancePerformance>(worker, "performance", JSON.stringify({ accountId: found.accountId }));
      return {
        accountId: found.accountId,
        cagr: performance.cagr,
        cagrUnavailable: performance.cagrUnavailable,
        sharpe: performance.sharpe,
        sharpeUnavailable: performance.sharpeUnavailable,
        sortino: performance.sortino,
        sortinoUnavailable: performance.sortinoUnavailable,
        maxDrawdown: performance.maxDrawdown,
        winRate: performance.winRate,
        sinceMs: performance.earliestReliableCheckpointMs,
      };
    } catch (error) {
      return workerFailure(reply, error, "performance_unavailable");
    }
  });

  // The executions already collected, narrowed and paged. Stored data
  // only: this one reaches no exchange.
  app.get<{ Querystring: Query }>(`${READ_ONLY_PREFIX}/account/trades`, async (request, reply) => {
    const found = await callerAndAccount(request, reply);
    if (!found) return reply;
    const symbol = request.query.symbol === undefined ? undefined : boundedString(request.query.symbol, 40);
    const cursor = request.query.cursor === undefined ? undefined : boundedString(request.query.cursor, 120);
    const since = epochMs(request.query.since);
    const until = epochMs(request.query.until);
    const limit = pageSize(request.query.limit);
    if (request.query.symbol !== undefined && symbol === undefined) return reply.code(400).send({ error: "symbol_invalid" });
    if (request.query.cursor !== undefined && cursor === undefined) return reply.code(400).send({ error: "cursor_invalid" });
    if (since === null || until === null || (since !== undefined && until !== undefined && since > until)) {
      return reply.code(400).send({ error: "period_invalid" });
    }
    if (limit === null) return reply.code(400).send({ error: "limit_invalid" });
    try {
      const page = await invokeWorker<ExecutedTradePage>(
        worker,
        "trades",
        JSON.stringify({ accountId: found.accountId, symbol, sinceMs: since, untilMs: until, cursor, limit }),
      );
      return { accountId: found.accountId, ...page };
    } catch (error) {
      return workerFailure(reply, error, "trades_unavailable");
    }
  });

  // The account's value and return over a range, for a chart.
  app.get<{ Querystring: Query }>(`${READ_ONLY_PREFIX}/account/series`, async (request, reply) => {
    const found = await callerAndAccount(request, reply);
    if (!found) return reply;
    const range = request.query.range ?? "max";
    if (!(SERIES_RANGES as readonly string[]).includes(range)) return reply.code(400).send({ error: "unknown_range" });
    try {
      const series = await invokeWorker<BinanceSeries>(
        worker,
        "series",
        JSON.stringify({ accountId: found.accountId, range: range as SeriesRange }),
      );
      return { accountId: found.accountId, range, ...series };
    } catch (error) {
      return workerFailure(reply, error, "series_unavailable");
    }
  });
}
