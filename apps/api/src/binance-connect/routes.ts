import { boundedString, boundedStrings } from "../security/input.js";
import type { RateLimiter } from "../auth/rateLimiter.js";
import type { FastifyInstance } from "fastify";
import { requireSession } from "../auth/requireSession.js";
import type { SessionStore } from "../auth/sessionStore.js";
import { invokeWorker, WorkerInvocationError } from "./worker.js";
import { SERIES_RANGES, type SeriesRange } from "./types.js";
import type { BinanceSeries, BinanceConnectionStatus, BinanceConnectRequest, CoinbaseConnectRequest, IbkrConnectRequest, ConnectedAccount, BinanceConnectResult, BinanceNav, BinancePerformance, BinancePerformanceProof } from "./types.js";

export interface BinanceConnectRouteOptions {
  sessionStore: SessionStore;
  /** accountId -> the only address allowed to connect/read this account's exchange link. */
  accountOwners: Map<string, `0x${string}`>;
  /** Real proving is CPU/RAM-heavy, so one session can only start it so often. */
  proofRateLimiter?: RateLimiter;
  now?: () => Date;
  /** Called once an account is removed, so anything cached about it is dropped. */
  onAccountRemoved?: (ownerAddress: `0x${string}`, accountId: string) => void;
  /** Path to the compiled `binance-worker` binary. */
  workerBinaryPath: string;
}

/** An address always owns the accountId equal to its own address, and
 * any `<address>_<name>` it adds for further exchange accounts — this
 * lets a signed-in person manage several Binance connections without
 * a prior, separate registration in `accountOwners` (that map still
 * takes precedence when it has an explicit entry). Never grants access
 * to any *other* person's accountId: the prefix must be the caller's
 * own address, and the name part is restricted to a short slug. */
const EXTRA_ACCOUNT_NAME = /^[A-Za-z0-9-]{1,32}$/;

export function isOwner(accountOwners: Map<string, `0x${string}`>, accountId: string, callerAddress: `0x${string}`): boolean {
  const registered = accountOwners.get(accountId);
  if (registered) {
    return registered.toLowerCase() === callerAddress.toLowerCase();
  }
  const caller = callerAddress.toLowerCase();
  const id = accountId.toLowerCase();
  if (id === caller) {
    return true;
  }
  return id.startsWith(`${caller}_`) && EXTRA_ACCOUNT_NAME.test(accountId.slice(callerAddress.length + 1));
}

/** Registers `apps/api/binance-connect`: the real
 * "connect your Binance account" flow — this is what makes the
 * connectivity `services/collector/binance/live-client` proved work
 * for any person, not just a manually-run test. Never logs or returns
 * the api key/secret; they go to the Rust worker binary via stdin
 * only and are stored encrypted at rest (`services/collector/binance/
 * worker`'s own scope). */
export function registerBinanceConnectRoutes(app: FastifyInstance, options: BinanceConnectRouteOptions): void {
  let proofInFlight = false;
  app.post<{ Params: { accountId: string }; Body: unknown }>("/accounts/:accountId/binance-connection", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    if (!isOwner(options.accountOwners, request.params.accountId, session.address)) {
      return reply.code(403).send({ error: "cross_account_access_denied" });
    }
    const body = (request.body ?? {}) as Record<string, unknown>;
    const apiKey = boundedString(body.apiKey, 256);
    const apiSecret = boundedString(body.apiSecret, 256);
    const symbols = body.symbols === undefined ? [] : boundedStrings(body.symbols, 200, 20);
    const label = body.label === undefined ? undefined : boundedString(body.label, 40);
    if (!apiKey || !apiSecret || !symbols || (body.label !== undefined && !label)) {
      return reply.code(400).send({ error: "apiKey and apiSecret are required" });
    }
    // symbols is optional: it only controls which pairs get a one-time
    // *historical* trade backfill at connect time (myTrades requires a
    // symbol per call). Every trade from this point forward is caught
    // in real time via the account's WebSocket user data stream,
    // regardless of symbol — no selection needed for that part.

    try {
      const result = await invokeWorker<BinanceConnectResult>(
        { binaryPath: options.workerBinaryPath },
        "connect",
        JSON.stringify({ accountId: request.params.accountId, apiKey, apiSecret, symbols, label }),
      );
      return reply.code(201).send(result);
    } catch (error) {
      if (error instanceof WorkerInvocationError) {
        // The worker's own error message never contains the raw key/
        // secret (see its `LiveClientError`/`RestrictionsError`
        // messages) — safe to forward as-is, e.g. "apiRestrictions
        // grants a write/trade/transfer capability: enableWithdrawals".
        return reply.code(422).send({ error: "binance_connection_failed", detail: error.message });
      }
      throw error;
    }
  });

  app.post<{ Params: { accountId: string }; Body: unknown }>("/accounts/:accountId/coinbase-connection", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    if (!isOwner(options.accountOwners, request.params.accountId, session.address)) {
      return reply.code(403).send({ error: "cross_account_access_denied" });
    }
    const body = (request.body ?? {}) as Record<string, unknown>;
    const keyName = boundedString(body.keyName, 256);
    // A Coinbase private key is a PEM block or a base64 secret: a few hundred bytes.
    const privateKey = boundedString(body.privateKey, 4096);
    const label = body.label === undefined ? undefined : boundedString(body.label, 40);
    if (!keyName || !privateKey || (body.label !== undefined && !label)) {
      return reply.code(400).send({ error: "keyName and privateKey are required" });
    }
    try {
      const result = await invokeWorker<BinanceConnectResult>(
        { binaryPath: options.workerBinaryPath },
        "connect",
        JSON.stringify({ exchange: "coinbase", accountId: request.params.accountId, keyName, privateKey, label }),
      );
      return reply.code(201).send(result);
    } catch (error) {
      if (error instanceof WorkerInvocationError) {
        return reply.code(422).send({ error: "coinbase_connection_failed", detail: error.message });
      }
      throw error;
    }
  });

  // Removes a connected account: its stored credential and everything collected
  // for it. What is already public is not a copy this service keeps, so it stops
  // showing as soon as the cache lapses (the cache is dropped here).
  app.delete<{ Params: { accountId: string } }>("/accounts/:accountId", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    if (!isOwner(options.accountOwners, request.params.accountId, session.address)) {
      return reply.code(403).send({ error: "cross_account_access_denied" });
    }
    try {
      const result = await invokeWorker<{ removed: boolean; broker: string }>(
        { binaryPath: options.workerBinaryPath },
        "disconnect",
        JSON.stringify({ accountId: request.params.accountId }),
      );
      if (!result.removed) {
        return reply.code(404).send({ error: "not_found" });
      }
      options.onAccountRemoved?.(session.address, request.params.accountId);
      return { removed: true, broker: result.broker };
    } catch (error) {
      if (error instanceof WorkerInvocationError) {
        if (error.message.startsWith("no connection for account")) {
          return reply.code(404).send({ error: "not_found" });
        }
        return reply.code(502).send({ error: "remove_failed", detail: error.message });
      }
      throw error;
    }
  });

  app.patch<{ Params: { accountId: string }; Body: { label?: string } }>("/accounts/:accountId/label", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    if (!isOwner(options.accountOwners, request.params.accountId, session.address)) {
      return reply.code(403).send({ error: "cross_account_access_denied" });
    }
    const label = boundedString((request.body as Record<string, unknown> | undefined)?.label, 40);
    if (!label) {
      return reply.code(400).send({ error: "label is required" });
    }
    try {
      return await invokeWorker({ binaryPath: options.workerBinaryPath }, "rename", JSON.stringify({ accountId: request.params.accountId, label }));
    } catch (error) {
      if (error instanceof WorkerInvocationError) {
        return reply.code(502).send({ error: "rename_failed", detail: error.message });
      }
      throw error;
    }
  });

  app.post<{ Params: { accountId: string }; Body: unknown }>("/accounts/:accountId/ibkr-connection", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    if (!isOwner(options.accountOwners, request.params.accountId, session.address)) {
      return reply.code(403).send({ error: "cross_account_access_denied" });
    }
    const body = (request.body ?? {}) as Record<string, unknown>;
    const token = boundedString(body.token, 256);
    const queryId = boundedString(body.queryId, 32);
    const label = body.label === undefined ? undefined : boundedString(body.label, 40);
    if (!token || !queryId || (body.label !== undefined && !label)) {
      return reply.code(400).send({ error: "token and queryId are required" });
    }
    try {
      const result = await invokeWorker<BinanceConnectResult>(
        { binaryPath: options.workerBinaryPath },
        "connect",
        JSON.stringify({ exchange: "ibkr", accountId: request.params.accountId, token, queryId, label }),
      );
      return reply.code(201).send(result);
    } catch (error) {
      if (error instanceof WorkerInvocationError) {
        return reply.code(422).send({ error: "ibkr_connection_failed", detail: error.message });
      }
      throw error;
    }
  });

  // "Refresh": pull the latest deposits/withdrawals from the exchange now.
  // Trades already arrive in real time; this is for everything the
  // periodic sync would otherwise only catch up on later.
  app.post<{ Params: { accountId: string } }>("/accounts/:accountId/binance-sync", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    if (!isOwner(options.accountOwners, request.params.accountId, session.address)) {
      return reply.code(403).send({ error: "cross_account_access_denied" });
    }
    try {
      return await invokeWorker({ binaryPath: options.workerBinaryPath }, "sync", JSON.stringify({ accountId: request.params.accountId }));
    } catch (error) {
      if (error instanceof WorkerInvocationError) {
        return reply.code(502).send({ error: "binance_sync_failed", detail: error.message });
      }
      throw error;
    }
  });

  // Every exchange account this identity has connected — what the
  // portfolio lists and adds up.
  app.get("/accounts", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    try {
      const { accounts } = await invokeWorker<{ accounts: ConnectedAccount[] }>(
        { binaryPath: options.workerBinaryPath },
        "list",
        JSON.stringify({ ownerAddress: session.address }),
      );
      return { accounts };
    } catch (error) {
      if (error instanceof WorkerInvocationError) {
        return reply.code(502).send({ error: "accounts_unavailable", detail: error.message });
      }
      throw error;
    }
  });

  app.get<{ Params: { accountId: string } }>("/accounts/:accountId/binance-connection", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    if (!isOwner(options.accountOwners, request.params.accountId, session.address)) {
      return reply.code(403).send({ error: "cross_account_access_denied" });
    }

    try {
      const status = await invokeWorker<BinanceConnectionStatus>(
        { binaryPath: options.workerBinaryPath },
        "status",
        JSON.stringify({ accountId: request.params.accountId }),
      );
      return status;
    } catch (error) {
      if (error instanceof WorkerInvocationError) {
        return reply.code(502).send({ error: "binance_status_unavailable", detail: error.message });
      }
      throw error;
    }
  });

  // Real, current NAV — not a historical series (no returns/Sharpe/
  // CAGR here yet; those need a real time series this connection is
  // still accumulating). Owner-only: unlike the 5-dimension public
  // projection, this exposes the actual computed value.
  app.get<{ Params: { accountId: string } }>("/accounts/:accountId/binance-nav", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    if (!isOwner(options.accountOwners, request.params.accountId, session.address)) {
      return reply.code(403).send({ error: "cross_account_access_denied" });
    }

    try {
      const nav = await invokeWorker<BinanceNav>({ binaryPath: options.workerBinaryPath }, "nav", JSON.stringify({ accountId: request.params.accountId }));
      return nav;
    } catch (error) {
      if (error instanceof WorkerInvocationError) {
        return reply.code(422).send({ error: "binance_nav_unavailable", detail: error.message });
      }
      throw error;
    }
  });

  // The chart: the account's value/return over 24h, 7d, 30d, 1y, 5y or
  // everything since it was connected, from real market candles.
  app.get<{ Params: { accountId: string }; Querystring: { range?: string } }>("/accounts/:accountId/binance-series", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    if (!isOwner(options.accountOwners, request.params.accountId, session.address)) {
      return reply.code(403).send({ error: "cross_account_access_denied" });
    }
    const range = request.query.range ?? "max";
    if (!(SERIES_RANGES as readonly string[]).includes(range)) {
      return reply.code(400).send({ error: "unknown_range" });
    }
    try {
      return await invokeWorker<BinanceSeries>(
        { binaryPath: options.workerBinaryPath },
        "series",
        JSON.stringify({ accountId: request.params.accountId, range: range as SeriesRange }),
      );
    } catch (error) {
      if (error instanceof WorkerInvocationError) {
        return reply.code(422).send({ error: "binance_series_unavailable", detail: error.message });
      }
      throw error;
    }
  });

  // Real historical performance: daily NAV/returns (chart data) plus
  // Sharpe/Sortino/MDD/CAGR/win-rate, each independently null with a
  // reason when its own minimum sample isn't met. Owner-only, and can
  // take on the order of a minute for an account with months of real
  // history (see services/collector/binance/worker's README on why —
  // a real, documented performance gap, not a bug).
  app.get<{ Params: { accountId: string } }>("/accounts/:accountId/binance-performance", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    if (!isOwner(options.accountOwners, request.params.accountId, session.address)) {
      return reply.code(403).send({ error: "cross_account_access_denied" });
    }

    try {
      const performance = await invokeWorker<BinancePerformance>(
        { binaryPath: options.workerBinaryPath },
        "performance",
        JSON.stringify({ accountId: request.params.accountId }),
      );
      return performance;
    } catch (error) {
      if (error instanceof WorkerInvocationError) {
        return reply.code(422).send({ error: "binance_performance_unavailable", detail: error.message });
      }
      throw error;
    }
  });

  // Real ZK proof of performance: runs actual RISC Zero proving (via
  // binance-worker's prove.rs) over this account's real, collected
  // history and returns a portable, independently-verifiable receipt
  // — never the raw trades/flows/balance/account identifier. This is
  // genuinely CPU/RAM-heavy (real proving, not dev-mode) and can take
  // a long time; owner-only, same as every other route here.
  app.get<{ Params: { accountId: string } }>("/accounts/:accountId/binance-performance-proof", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    if (!isOwner(options.accountOwners, request.params.accountId, session.address)) {
      return reply.code(403).send({ error: "cross_account_access_denied" });
    }

    if (options.proofRateLimiter && !options.proofRateLimiter.allow(session.address.toLowerCase(), (options.now?.() ?? new Date()).getTime())) {
      return reply.code(429).send({ error: "rate_limited" });
    }

    // A real proof takes minutes and gigabytes: one at a time, machine-wide.
    if (proofInFlight) {
      return reply.code(503).send({ error: "proof_busy" });
    }
    proofInFlight = true;
    try {
      const proof = await invokeWorker<BinancePerformanceProof>(
        { binaryPath: options.workerBinaryPath },
        "prove-performance",
        JSON.stringify({ accountId: request.params.accountId }),
      );
      return proof;
    } catch (error) {
      if (error instanceof WorkerInvocationError) {
        return reply.code(422).send({ error: "binance_performance_proof_unavailable", detail: error.message });
      }
      throw error;
    } finally {
      proofInFlight = false;
    }
  });
}
