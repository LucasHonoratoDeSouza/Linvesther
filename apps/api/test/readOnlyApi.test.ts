import { chmodSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { buildApp } from "../src/app.js";
import { MemorySessionStore } from "../src/auth/sessionStore.js";
import { MemoryReadOnlyTokenStore, readOnlyTokenDigest } from "../src/auth/readOnlyTokenStore.js";
import { readOnlyTrafficKey } from "../src/auth/requireReadOnlyToken.js";
import type { RateLimit } from "../src/auth/rateLimiter.js";
import type { FastifyRequest } from "fastify";
import { sumDecimals } from "../src/readonly/totals.js";

// The read-only API is driven through the real Fastify app against a
// small fake "worker binary" — the same stdin-JSON-in, stdout-JSON-out
// contract the Rust binary implements — so this suite runs fast and
// needs neither a Rust build nor a live Postgres. Every fake answer
// echoes the request it was given, which is how a test can tell *which*
// account was actually read.

const ME = "0x1111111111111111111111111111111111111111" as const;
const SOMEONE_ELSE = "0x2222222222222222222222222222222222222222" as const;

const PERFORMANCE = {
  dailyNav: [{ dayStartMs: 1, nav: "100" }],
  dailyReturns: ["0.01"],
  sharpe: "1.25",
  sharpeUnavailable: null,
  sortino: null,
  sortinoUnavailable: "not enough days below the mean yet",
  maxDrawdown: "0.08",
  cagr: null,
  cagrUnavailable: "a year has not passed since the connection",
  winRate: { closedRoundTrips: 4, wins: 3, winRate: "0.75" },
  earliestReliableCheckpointMs: 1700000000000,
  pnl: {
    positions: [{ asset: "BTC", quantity: "0.5", averageCost: "40000", markPrice: "60000", realized: "10", unrealized: "10000" }],
    realized: "10",
    unrealized: "10000",
    fees: "1.5",
  },
};

const TRADES = {
  trades: [
    { symbol: "BTCUSDT", tradeId: 1, orderId: 10, price: "40000", quantity: "0.1", commission: "0.4", commissionAsset: "USDT", timeMs: 1700000000000, side: "buy" },
  ],
  nextCursor: "1700000000000:1:BTCUSDT",
  sinceMs: 1699000000000,
};

const SERIES = { points: [{ timeMs: 1, nav: "100", index: "1" }], stepMs: 3_600_000, sinceMs: 1 };

/** A worker that answers every read this API makes and records what it
 * was asked for. `nav` lets a test give each account its own value, or
 * make one of them fail. `list` filters by owner the way the real
 * worker's `connections_owned_by` does, so a test can tell whether a
 * route asked about the right identity. */
function fakeWorker(options: { accounts?: unknown[]; nav?: Record<string, string | { error: string }>; defaultNav?: string } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "read-only-worker-"));
  const path = join(dir, "worker.mjs");
  const log = join(dir, "calls.log");
  const accounts = options.accounts ?? [{ accountId: ME, broker: "binance", label: "Main", connectedAtMs: 1, status: "active" }];
  writeFileSync(
    path,
    `#!/usr/bin/env node
import { appendFileSync, readFileSync } from "node:fs";
const subcommand = process.argv[2];
const input = JSON.parse(readFileSync(0, "utf8") || "{}");
appendFileSync(${JSON.stringify(log)}, subcommand + "\\n");
const accounts = ${JSON.stringify(accounts)};
const navByAccount = ${JSON.stringify(options.nav ?? {})};
const answer = (body) => { console.log(JSON.stringify({ ok: true, echoed: { subcommand, input }, ...body })); };
const refuse = (message) => { console.log(JSON.stringify({ ok: false, error: message })); process.exit(1); };
const ownedBy = (owner) => accounts.filter((a) => {
  const id = a.accountId.toLowerCase();
  return id === owner.toLowerCase() || id.startsWith(owner.toLowerCase() + "_");
});
switch (subcommand) {
  case "list": answer({ accounts: ownedBy(input.ownerAddress) }); break;
  case "status": answer({ connected: true, status: "active", last_synced_at: "2026-09-01T00:00:00Z", trade_count: 7, flow_count: 2 }); break;
  case "nav": {
    const configured = navByAccount[input.accountId] ?? ${JSON.stringify(options.defaultNav ?? "1234.50")};
    if (configured && configured.error) { refuse(configured.error); break; }
    answer({ nav: configured, currency: "USDT", assets: [{ asset: "BTC", quantity: "0.5", price: "60000", value: "30000" }], excludedOutOfScopeAssets: [] });
    break;
  }
  case "performance": answer(${JSON.stringify(PERFORMANCE)}); break;
  case "trades": answer(${JSON.stringify(TRADES)}); break;
  case "series": answer(${JSON.stringify(SERIES)}); break;
  default: refuse("this worker was asked for " + subcommand + ", which the read-only API must never call");
}
`,
  );
  chmodSync(path, 0o755);
  writeFileSync(log, "");
  return { path, calls: () => readFileSync(log, "utf8").split("\n").filter(Boolean) };
}

async function appWithToken(workerOptions: Parameters<typeof fakeWorker>[0] = {}, appOverrides: Partial<Parameters<typeof buildApp>[0]> = {}) {
  const sessionStore = new MemorySessionStore();
  const readOnlyTokenStore = new MemoryReadOnlyTokenStore();
  const worker = fakeWorker(workerOptions);
  const app = buildApp({
    domain: "localhost",
    sessionStore,
    readOnlyTokenStore,
    binanceWorkerBinaryPath: worker.path,
    ...appOverrides,
  });
  await app.ready();
  const mine = await readOnlyTokenStore.issue(ME, "Hermes");
  const theirs = await readOnlyTokenStore.issue(SOMEONE_ELSE, "their agent");
  const session = await sessionStore.create(ME);
  return { app, readOnlyTokenStore, worker, token: mine.token, tokenRecord: mine.record, otherToken: theirs.token, cookies: { sid: session.id } };
}

const auth = (token: string) => ({ authorization: `Bearer ${token}` });

const ROUTES = [
  "/mcp/account/state",
  "/mcp/account/portfolio",
  "/mcp/account/nav",
  "/mcp/account/performance",
  "/mcp/account/trades",
  "/mcp/account/series",
] as const;

describe("reading an account with a read-only token", () => {
  it("answers every read with the figures the worker computed", async () => {
    const { app, token } = await appWithToken();

    const state = await app.inject({ method: "GET", url: "/mcp/account/state", headers: auth(token) });
    expect(state.statusCode).toBe(200);
    expect(state.json()).toMatchObject({
      address: ME,
      scope: "read:account",
      accounts: [{ accountId: ME, broker: "binance", connection: { trade_count: 7 }, balance: { nav: "1234.50", currency: "USDT" } }],
      balance: { total: "1234.50", currency: "USDT" },
      balanceUnavailable: null,
    });

    const nav = await app.inject({ method: "GET", url: "/mcp/account/nav", headers: auth(token) });
    expect(nav.json()).toMatchObject({ accountId: ME, nav: "1234.50", currency: "USDT" });

    const portfolio = await app.inject({ method: "GET", url: "/mcp/account/portfolio", headers: auth(token) });
    expect(portfolio.json()).toMatchObject({ accountId: ME, positions: PERFORMANCE.pnl.positions, realized: "10", unrealized: "10000", fees: "1.5" });

    const performance = await app.inject({ method: "GET", url: "/mcp/account/performance", headers: auth(token) });
    expect(performance.json()).toMatchObject({ sharpe: "1.25", sortino: null, sortinoUnavailable: "not enough days below the mean yet", maxDrawdown: "0.08", winRate: { winRate: "0.75" } });

    const trades = await app.inject({ method: "GET", url: "/mcp/account/trades", headers: auth(token) });
    expect(trades.json()).toMatchObject({ accountId: ME, trades: TRADES.trades, nextCursor: TRADES.nextCursor });

    const series = await app.inject({ method: "GET", url: "/mcp/account/series?range=7d", headers: auth(token) });
    expect(series.json()).toMatchObject({ accountId: ME, range: "7d", stepMs: 3_600_000 });
  });

  // A metric with no sample yet comes back null with its own reason;
  // nothing here may turn that into a number.
  it("passes an unavailable metric through as unavailable, never as zero", async () => {
    const { app, token } = await appWithToken();
    const body = (await app.inject({ method: "GET", url: "/mcp/account/performance", headers: auth(token) })).json();
    expect(body.cagr).toBeNull();
    expect(body.cagrUnavailable).toBe("a year has not passed since the connection");
  });

  it("reads the token owner's own account when none is named", async () => {
    const { app, token } = await appWithToken();
    const body = (await app.inject({ method: "GET", url: "/mcp/account/nav", headers: auth(token) })).json();
    expect(body.echoed.input.accountId).toBe(ME);
  });

  it("reads a further account of the same identity when named", async () => {
    const { app, token } = await appWithToken();
    const body = (await app.inject({ method: "GET", url: `/mcp/account/nav?accountId=${ME}_swing`, headers: auth(token) })).json();
    expect(body.echoed.input.accountId).toBe(`${ME}_swing`);
  });

  it("reports the whole balance exactly, adding decimals without rounding", async () => {
    const { app, token } = await appWithToken({
      accounts: [
        { accountId: ME, broker: "binance", label: "Main", connectedAtMs: 1, status: "active" },
        { accountId: `${ME}_swing`, broker: "kraken", label: "Swing", connectedAtMs: 2, status: "active" },
      ],
      nav: { [ME]: "0.1", [`${ME}_swing`]: "0.2" },
    });
    const body = (await app.inject({ method: "GET", url: "/mcp/account/state", headers: auth(token) })).json();
    expect(body.balance).toEqual({ total: "0.3", currency: "USDT" });
  });

  // Adding up what could be read and calling it the balance would read
  // as a real figure while being wrong.
  it("refuses the whole balance when one account's value cannot be read", async () => {
    const { app, token } = await appWithToken({
      accounts: [
        { accountId: ME, broker: "binance", label: "Main", connectedAtMs: 1, status: "active" },
        { accountId: `${ME}_swing`, broker: "kraken", label: "Swing", connectedAtMs: 2, status: "active" },
      ],
      nav: { [ME]: "100", [`${ME}_swing`]: { error: "no price for one of the held assets" } },
    });
    const body = (await app.inject({ method: "GET", url: "/mcp/account/state", headers: auth(token) })).json();
    expect(body.balance).toBeNull();
    expect(body.balanceUnavailable).toContain("no price for one of the held assets");
    // The account that could be read still reports its own value.
    expect(body.accounts[0].balance).toEqual({ nav: "100", currency: "USDT" });
    expect(body.accounts[1].balance).toBeNull();
  });

  it("says so when the identity has connected nothing at all", async () => {
    const { app, token } = await appWithToken({ accounts: [] });
    const body = (await app.inject({ method: "GET", url: "/mcp/account/state", headers: auth(token) })).json();
    expect(body.accounts).toEqual([]);
    expect(body.balance).toBeNull();
    expect(body.balanceUnavailable).toBe("no account is connected");
  });
});

describe("one token reaches exactly one identity's data", () => {
  it("refuses another identity's account on every route", async () => {
    const { app, otherToken } = await appWithToken();
    for (const route of ROUTES.filter((r) => r !== "/mcp/account/state")) {
      const response = await app.inject({ method: "GET", url: `${route}?accountId=${ME}`, headers: auth(otherToken) });
      expect(response.statusCode, route).toBe(403);
      expect(response.json().error, route).toBe("cross_account_access_denied");
    }
  });

  it("refuses an accountId that only looks like the caller's own", async () => {
    const { app, token, worker } = await appWithToken();
    for (const accountId of [SOMEONE_ELSE, `${SOMEONE_ELSE}_swing`, `${ME}x`, `${ME}_../${SOMEONE_ELSE}`, `${ME}_${"a".repeat(33)}`, ` ${SOMEONE_ELSE}`]) {
      const response = await app.inject({ method: "GET", url: `/mcp/account/nav?accountId=${encodeURIComponent(accountId)}`, headers: auth(token) });
      // Refused as somebody else's, or refused as not an accountId at
      // all — never read.
      expect([400, 403], accountId).toContain(response.statusCode);
    }
    expect(worker.calls()).toEqual([]);
  });

  it("lists only the accounts of the identity the token belongs to", async () => {
    const { app, token, otherToken } = await appWithToken();
    const mine = (await app.inject({ method: "GET", url: "/mcp/account/state", headers: auth(token) })).json();
    expect(mine.accounts.map((a: { accountId: string }) => a.accountId)).toEqual([ME]);

    const theirs = (await app.inject({ method: "GET", url: "/mcp/account/state", headers: auth(otherToken) })).json();
    expect(theirs.address).toBe(SOMEONE_ELSE);
    expect(theirs.accounts).toEqual([]);
  });
});

describe("what does not authenticate a read", () => {
  it("nothing at all", async () => {
    const { app } = await appWithToken();
    for (const route of ROUTES) {
      const response = await app.inject({ method: "GET", url: route });
      expect(response.statusCode, route).toBe(401);
      expect(response.json().error, route).toBe("unauthenticated");
    }
  });

  it("a revoked token, on every route", async () => {
    const { app, readOnlyTokenStore, token, tokenRecord } = await appWithToken();
    expect((await app.inject({ method: "GET", url: "/mcp/account/nav", headers: auth(token) })).statusCode).toBe(200);
    await readOnlyTokenStore.revoke(ME, tokenRecord.id, new Date());
    for (const route of ROUTES) {
      const response = await app.inject({ method: "GET", url: route, headers: auth(token) });
      expect(response.statusCode, route).toBe(401);
      expect(response.json().error, route).toBe("token_revoked");
    }
  });

  // The browser sends its session cookie to this origin on every
  // request. It must never be what answers an agent's route.
  it("a signed-in browser session", async () => {
    const { app, cookies } = await appWithToken();
    for (const route of ROUTES) {
      const response = await app.inject({ method: "GET", url: route, cookies });
      expect(response.statusCode, route).toBe(401);
    }
  });

  it("a token presented as a cookie rather than a header", async () => {
    const { app, token } = await appWithToken();
    const response = await app.inject({ method: "GET", url: "/mcp/account/nav", cookies: { sid: token } });
    expect(response.statusCode).toBe(401);
  });
});

// The whole point of this credential: it reads, and it cannot do
// anything else. Every route that changes state must refuse it.
describe("a read-only token changes nothing", () => {
  const writes = [
    { method: "POST" as const, url: `/accounts/${ME}/binance-connection`, payload: { apiKey: "k", apiSecret: "s" } },
    { method: "POST" as const, url: `/accounts/${ME}/coinbase-connection`, payload: { keyName: "k", privateKey: "p" } },
    { method: "POST" as const, url: `/accounts/${ME}/kraken-connection`, payload: { apiKey: "k", apiSecret: "s" } },
    { method: "POST" as const, url: `/accounts/${ME}/ibkr-connection`, payload: { token: "t", queryId: "q" } },
    { method: "POST" as const, url: `/accounts/${ME}/wallet-challenge`, payload: { address: ME, chainId: 1 } },
    { method: "POST" as const, url: `/accounts/${ME}/wallet-connection`, payload: { message: "m", signature: "s" } },
    { method: "POST" as const, url: `/accounts/${ME}/binance-sync`, payload: {} },
    { method: "PATCH" as const, url: `/accounts/${ME}/label`, payload: { label: "renamed" } },
    { method: "DELETE" as const, url: `/accounts/${ME}`, payload: undefined },
    { method: "PUT" as const, url: "/profile/settings", payload: { privacyMode: true } },
    { method: "POST" as const, url: "/api-tokens", payload: { label: "a token minting a token" } },
    { method: "DELETE" as const, url: "/api-tokens/any-id", payload: undefined },
  ];

  it.each(writes)("refuses $method $url", async ({ method, url, payload }) => {
    const { app, token } = await appWithToken();
    const response = await app.inject({ method, url, headers: auth(token), payload });
    expect(response.statusCode).toBe(401);
    expect(response.json().error).toBe("unauthenticated");
  });

  it("refuses the owner-only reads that belong to the browser session", async () => {
    const { app, token } = await appWithToken();
    for (const url of ["/accounts", `/accounts/${ME}/binance-nav`, `/accounts/${ME}/binance-performance`, "/api-tokens", "/auth/session"]) {
      const response = await app.inject({ method: "GET", url, headers: auth(token) });
      expect(response.statusCode, url).toBe(401);
    }
  });

  it("never asks the worker for anything but a read", async () => {
    const { app, token, worker } = await appWithToken();
    for (const route of ROUTES) {
      expect((await app.inject({ method: "GET", url: route, headers: auth(token) })).statusCode, route).toBe(200);
    }
    const asked = new Set(worker.calls());
    expect([...asked].sort()).toEqual(["list", "nav", "performance", "series", "status", "trades"]);
    for (const forbidden of ["connect", "disconnect", "rename", "sync", "prove-performance", "rekey"]) {
      expect(asked.has(forbidden), forbidden).toBe(false);
    }
  });
});

describe("what a read-only route refuses to look up", () => {
  it("a range it does not have", async () => {
    const { app, token } = await appWithToken();
    const response = await app.inject({ method: "GET", url: "/mcp/account/series?range=3d", headers: auth(token) });
    expect(response.statusCode).toBe(400);
    expect(response.json().error).toBe("unknown_range");
  });

  it("a period, page size or cursor that is not one", async () => {
    const { app, token } = await appWithToken();
    const cases: [string, string][] = [
      ["since=yesterday", "period_invalid"],
      ["until=-1", "period_invalid"],
      ["since=2&until=1", "period_invalid"],
      ["limit=0", "limit_invalid"],
      ["limit=501", "limit_invalid"],
      ["limit=ten", "limit_invalid"],
      [`cursor=${"c".repeat(200)}`, "cursor_invalid"],
      ["symbol=", "symbol_invalid"],
    ];
    for (const [query, error] of cases) {
      const response = await app.inject({ method: "GET", url: `/mcp/account/trades?${query}`, headers: auth(token) });
      expect(response.statusCode, query).toBe(400);
      expect(response.json().error, query).toBe(error);
    }
  });

  it("passes the filters it accepts straight through to the worker", async () => {
    const { app, token } = await appWithToken();
    const body = (await app.inject({ method: "GET", url: "/mcp/account/trades?symbol=BTCUSDT&since=1000&until=2000&limit=50&cursor=1000:1:BTCUSDT", headers: auth(token) })).json();
    expect(body.echoed.input).toMatchObject({ symbol: "BTCUSDT", sinceMs: 1000, untilMs: 2000, limit: 50, cursor: "1000:1:BTCUSDT" });
  });

  it("reports a worker failure as a failure, with the worker's own reason", async () => {
    const { app, token } = await appWithToken({ nav: { [ME]: { error: "no connection for account" } } });
    const response = await app.inject({ method: "GET", url: "/mcp/account/nav", headers: auth(token) });
    expect(response.statusCode).toBe(422);
    expect(response.json()).toMatchObject({ error: "nav_unavailable", detail: "no connection for account" });
  });
});

describe("how much an agent may read", () => {
  /** Records what the traffic limiter was asked, and refuses only the
   * read-only bucket, so one request is enough to see the answer. */
  function recordingLimits() {
    const asked: { bucket: string; max: number; windowMs: number; key: string }[] = [];
    const factory = (bucket: string, max: number, windowMs: number): RateLimit => ({
      allow(key: string) {
        asked.push({ bucket, max, windowMs, key });
        return bucket !== "read-only-api";
      },
    });
    return { asked, factory };
  }

  it("is capped per minute, and the cap is tighter than a browser's reads", async () => {
    const { asked, factory } = recordingLimits();
    const { app, token } = await appWithToken({}, { limiterFactory: factory });
    const response = await app.inject({ method: "GET", url: "/mcp/account/nav", headers: auth(token) });
    expect(response.statusCode).toBe(429);
    const bucket = asked.find((entry) => entry.bucket === "read-only-api");
    expect(bucket).toMatchObject({ max: 60, windowMs: 60_000 });
  });

  it("counts per token, not per identity or per address", () => {
    const request = (headers: Record<string, string>) => ({ headers, ip: "203.0.113.7" }) as unknown as FastifyRequest;
    const a = "lvz_ro_" + "a".repeat(64);
    const b = "lvz_ro_" + "b".repeat(64);
    expect(readOnlyTrafficKey(request({ authorization: `Bearer ${a}` }))).toBe(readOnlyTrafficKey(request({ authorization: `Bearer ${a}` })));
    expect(readOnlyTrafficKey(request({ authorization: `Bearer ${a}` }))).not.toBe(readOnlyTrafficKey(request({ authorization: `Bearer ${b}` })));
  });

  it("counts against the token's hash, so no limit table holds a usable credential", () => {
    const token = "lvz_ro_" + "a".repeat(64);
    const key = readOnlyTrafficKey({ headers: { authorization: `Bearer ${token}` }, ip: "203.0.113.7" } as unknown as FastifyRequest);
    expect(key).not.toContain(token);
    expect(key).toContain(readOnlyTokenDigest(token));
  });

  it("falls back to the client address when there is no token to count", () => {
    const key = readOnlyTrafficKey({ headers: {}, ip: "203.0.113.7" } as unknown as FastifyRequest);
    expect(key).toBe("address:203.0.113.7");
  });
});

describe("handing out and taking back a token", () => {
  it("shows the token once, and never again", async () => {
    const { app, cookies } = await appWithToken();
    const created = await app.inject({ method: "POST", url: "/api-tokens", cookies, payload: { label: "Hermes" } });
    expect(created.statusCode).toBe(201);
    const { token, record } = created.json();
    expect(token).toMatch(/^lvz_ro_[0-9a-f]{64}$/);
    expect(record).toMatchObject({ label: "Hermes", scope: "read:account", lastUsedAt: null, revokedAt: null });

    const listed = await app.inject({ method: "GET", url: "/api-tokens", cookies });
    expect(listed.statusCode).toBe(200);
    expect(JSON.stringify(listed.json())).not.toContain(token);
    expect(listed.json().tokens.find((t: { id: string }) => t.id === record.id)).toBeDefined();
  });

  it("the token it hands out reads that identity's account straight away", async () => {
    const { app, cookies } = await appWithToken();
    const { token } = (await app.inject({ method: "POST", url: "/api-tokens", cookies, payload: { label: "Hermes" } })).json();
    const read = await app.inject({ method: "GET", url: "/mcp/account/nav", headers: auth(token) });
    expect(read.statusCode).toBe(200);
    expect(read.json()).toMatchObject({ accountId: ME });
  });

  it("records when a token was last used, so an unused one can be spotted and revoked", async () => {
    const { app, cookies, token, tokenRecord } = await appWithToken();
    expect((await app.inject({ method: "GET", url: "/api-tokens", cookies })).json().tokens.find((t: { id: string }) => t.id === tokenRecord.id).lastUsedAt).toBeNull();
    await app.inject({ method: "GET", url: "/mcp/account/nav", headers: auth(token) });
    expect((await app.inject({ method: "GET", url: "/api-tokens", cookies })).json().tokens.find((t: { id: string }) => t.id === tokenRecord.id).lastUsedAt).not.toBeNull();
  });

  it("revoking stops the token immediately", async () => {
    const { app, cookies, token, tokenRecord } = await appWithToken();
    expect((await app.inject({ method: "GET", url: "/mcp/account/nav", headers: auth(token) })).statusCode).toBe(200);
    const revoked = await app.inject({ method: "DELETE", url: `/api-tokens/${tokenRecord.id}`, cookies });
    expect(revoked.statusCode).toBe(200);
    expect((await app.inject({ method: "GET", url: "/mcp/account/nav", headers: auth(token) })).statusCode).toBe(401);
  });

  it("refuses to revoke a token belonging to someone else, the same way it refuses one that does not exist", async () => {
    const { app, cookies, readOnlyTokenStore } = await appWithToken();
    const theirs = (await readOnlyTokenStore.list(SOMEONE_ELSE))[0]!;
    const mistaken = await app.inject({ method: "DELETE", url: `/api-tokens/${theirs.id}`, cookies });
    const missing = await app.inject({ method: "DELETE", url: "/api-tokens/no-such-token", cookies });
    expect(mistaken.statusCode).toBe(404);
    expect(mistaken.json()).toEqual(missing.json());
    expect((await readOnlyTokenStore.list(SOMEONE_ELSE))[0]!.revokedAt).toBeNull();
  });

  it("needs a name, and refuses one that is not text", async () => {
    const { app, cookies } = await appWithToken();
    for (const label of [undefined, "", "   ", 7, {}, "a".repeat(41)]) {
      const response = await app.inject({ method: "POST", url: "/api-tokens", cookies, payload: { label } });
      expect(response.statusCode, JSON.stringify(label)).toBe(400);
    }
  });

  it("lists only the signed-in identity's tokens", async () => {
    const { app, cookies } = await appWithToken();
    const { tokens } = (await app.inject({ method: "GET", url: "/api-tokens", cookies })).json();
    expect(tokens.every((t: { address: string }) => t.address.toLowerCase() === ME.toLowerCase())).toBe(true);
  });

  it("needs a session: an unauthenticated caller cannot list or mint", async () => {
    const { app } = await appWithToken();
    expect((await app.inject({ method: "GET", url: "/api-tokens" })).statusCode).toBe(401);
    expect((await app.inject({ method: "POST", url: "/api-tokens", payload: { label: "x" } })).statusCode).toBe(401);
  });
});

describe("adding money", () => {
  it("adds decimals exactly, at the widest scale any figure carried", () => {
    expect(sumDecimals(["0.1", "0.2"])).toBe("0.3");
    expect(sumDecimals(["1", "2"])).toBe("3");
    expect(sumDecimals(["1.005", "2.1"])).toBe("3.105");
    expect(sumDecimals(["-1.50", "0.25"])).toBe("-1.25");
    expect(sumDecimals(["0.00000001", "0.00000002"])).toBe("0.00000003");
    expect(sumDecimals([])).toBe("0");
  });

  it("keeps precision a double would lose", () => {
    expect(sumDecimals(["9007199254740993.01", "0.02"])).toBe("9007199254740993.03");
    expect(Number("0.1") + Number("0.2")).not.toBe(0.3);
  });

  it("refuses a figure that is not a plain decimal rather than skipping it", () => {
    for (const bad of ["", "abc", "1e5", "1,5", "NaN", "Infinity", "0x10", "1.2.3"]) {
      expect(sumDecimals(["1", bad]), bad).toBeNull();
    }
  });
});
