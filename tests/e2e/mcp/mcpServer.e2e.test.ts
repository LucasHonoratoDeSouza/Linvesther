import { chmodSync, mkdtempSync, writeFileSync } from "node:fs";
import type { Server } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { buildApp, MemoryReadOnlyTokenStore } from "@linvestherzk/api";
import { createHttpServer, MCP_PATH } from "@linvestherzk/mcp-server";
import { afterEach, describe, expect, it } from "vitest";

// End-to-end: an agent holding a read-only token reads an account
// through the whole real path — a real MCP client, over real HTTP, to
// the real MCP server, which calls a real `apps/api` instance listening
// on a real port, which spawns a worker binary.
//
// The only stand-in is that worker binary: a script implementing the
// same stdin-JSON-in, stdout-JSON-out contract the Rust one does, so this
// runs in the default gate without a Rust build, a Postgres or exchange
// credentials. Everything above it — the protocol handshake, the tool
// schemas, the bearer token, the ownership check, the HTTP routes — is
// the real thing.
//
// The credentialed path against a real exchange is covered by
// `tests/e2e/binance-connect/binanceConnect.e2e.test.ts`.

const ME = "0x1111111111111111111111111111111111111111" as const;
const SOMEONE_ELSE = "0x2222222222222222222222222222222222222222" as const;

const PERFORMANCE = {
  dailyNav: [{ dayStartMs: 1, nav: "100" }],
  dailyReturns: ["0.01"],
  sharpe: "1.4",
  sharpeUnavailable: null,
  sortino: null,
  sortinoUnavailable: "not enough days below the mean yet",
  maxDrawdown: "0.12",
  cagr: null,
  cagrUnavailable: "a year has not passed since the connection",
  winRate: { closedRoundTrips: 6, wins: 4, winRate: "0.6666" },
  earliestReliableCheckpointMs: 1_700_000_000_000,
  pnl: {
    positions: [{ asset: "ETH", quantity: "3", averageCost: "2000", markPrice: "2500", realized: "12.5", unrealized: "1500" }],
    realized: "12.5",
    unrealized: "1500",
    fees: "3.25",
  },
};

/** The worker contract, answered by a script. Every subcommand the
 * read-only routes use is here; anything else is refused loudly, so a
 * write reaching the worker through this path would fail the test rather
 * than pass unnoticed. */
function fakeWorkerBinary(): string {
  const dir = mkdtempSync(join(tmpdir(), "mcp-e2e-worker-"));
  const path = join(dir, "worker.mjs");
  writeFileSync(
    path,
    `#!/usr/bin/env node
import { readFileSync } from "node:fs";
const subcommand = process.argv[2];
const input = JSON.parse(readFileSync(0, "utf8") || "{}");
const answer = (body) => console.log(JSON.stringify({ ok: true, ...body }));
const accounts = [{ accountId: ${JSON.stringify(ME)}, broker: "binance", label: "Main", connectedAtMs: 1700000000000, status: "active" }];
switch (subcommand) {
  case "list":
    answer({ accounts: accounts.filter((a) => a.accountId.toLowerCase() === String(input.ownerAddress).toLowerCase()) });
    break;
  case "status":
    answer({ connected: true, status: "active", last_synced_at: "2026-09-01T00:00:00Z", trade_count: 12, flow_count: 3 });
    break;
  case "nav":
    answer({ nav: "7500.25", currency: "USDT", assets: [{ asset: "ETH", quantity: "3", price: "2500", value: "7500" }], excludedOutOfScopeAssets: [] });
    break;
  case "performance":
    answer(${JSON.stringify(PERFORMANCE)});
    break;
  case "trades":
    answer({
      trades: [
        { symbol: "ETHUSDT", tradeId: 1, orderId: 11, price: "2000", quantity: "1", commission: "0.2", commissionAsset: "USDT", timeMs: 1700000000000, side: "buy" },
        { symbol: "ETHUSDT", tradeId: 2, orderId: 12, price: "2500", quantity: "1", commission: "0.25", commissionAsset: "USDT", timeMs: 1700000100000, side: "sell" },
      ].filter((t) => (input.symbol ? t.symbol === input.symbol : true)),
      nextCursor: null,
      sinceMs: 1699000000000,
    });
    break;
  case "series":
    answer({ points: [{ timeMs: 1700000000000, nav: "7000", index: "1" }, { timeMs: 1700003600000, nav: "7500.25", index: "1.0715" }], stepMs: 3600000, sinceMs: 1699000000000 });
    break;
  default:
    console.log(JSON.stringify({ ok: false, error: "the read-only path must never ask for " + subcommand }));
    process.exit(1);
}
`,
  );
  chmodSync(path, 0o755);
  return path;
}

const closers: (() => Promise<void>)[] = [];

afterEach(async () => {
  while (closers.length > 0) await closers.pop()!();
});

const closeHttp = (server: Server) => () => new Promise<void>((resolve) => server.close(() => resolve()));

/** A real API on a port, a real MCP server in front of it, and a token
 * for each of two identities. */
async function stack() {
  const readOnlyTokenStore = new MemoryReadOnlyTokenStore();
  const api = buildApp({ domain: "localhost", readOnlyTokenStore, binanceWorkerBinaryPath: fakeWorkerBinary() });
  await api.listen({ port: 0, host: "127.0.0.1" });
  closers.push(() => api.close());
  const apiPort = (api.server.address() as AddressInfo).port;

  const mcp = createHttpServer({ baseUrl: `http://127.0.0.1:${apiPort}` });
  await new Promise<void>((resolve) => mcp.listen(0, "127.0.0.1", resolve));
  closers.push(closeHttp(mcp));
  const mcpUrl = new URL(`http://127.0.0.1:${(mcp.address() as AddressInfo).port}${MCP_PATH}`);

  const mine = await readOnlyTokenStore.issue(ME, "Hermes");
  const theirs = await readOnlyTokenStore.issue(SOMEONE_ELSE, "their agent");
  return {
    readOnlyTokenStore,
    mcpUrl,
    apiOrigin: `http://127.0.0.1:${apiPort}`,
    token: mine.token,
    tokenId: mine.record.id,
    otherToken: theirs.token,
  };
}

async function agent(url: URL, token: string) {
  const client = new Client({ name: "linvestherzk-e2e-agent", version: "0.0.0" });
  await client.connect(new StreamableHTTPClientTransport(url, { requestInit: { headers: { authorization: `Bearer ${token}` } } }));
  closers.push(() => client.close());
  return client;
}

describe("an agent reading an account through MCP", () => {
  it("connects, finds the six read-only tools, and gets a real answer from each", async () => {
    const { mcpUrl, token } = await stack();
    const client = await agent(mcpUrl, token);

    const { tools } = await client.listTools();
    expect(tools.map((t) => t.name).sort()).toEqual([
      "get_account_state",
      "get_chart_series",
      "get_nav",
      "get_performance",
      "get_portfolio",
      "get_trades",
    ]);
    expect(tools.every((t) => t.annotations?.readOnlyHint === true)).toBe(true);

    const state = await client.callTool({ name: "get_account_state", arguments: {} });
    expect(state.structuredContent).toMatchObject({
      address: ME,
      scope: "read:account",
      accounts: [{ accountId: ME, broker: "binance", connection: { connected: true, tradeCount: 12 }, balance: { nav: "7500.25", currency: "USDT" } }],
      balance: { total: "7500.25", currency: "USDT" },
    });

    const nav = await client.callTool({ name: "get_nav", arguments: {} });
    expect(nav.structuredContent).toMatchObject({ accountId: ME, nav: "7500.25", currency: "USDT", assets: [{ asset: "ETH", value: "7500" }] });

    const portfolio = await client.callTool({ name: "get_portfolio", arguments: {} });
    expect(portfolio.structuredContent).toMatchObject({ positions: [{ asset: "ETH", averageCost: "2000", unrealized: "1500" }], realized: "12.5", fees: "3.25" });

    const performance = await client.callTool({ name: "get_performance", arguments: {} });
    expect(performance.structuredContent).toMatchObject({
      sharpe: "1.4",
      maxDrawdown: "0.12",
      cagr: null,
      cagrUnavailable: "a year has not passed since the connection",
      winRate: { wins: 4 },
    });

    const trades = await client.callTool({ name: "get_trades", arguments: { symbol: "ETHUSDT", limit: 10 } });
    expect((trades.structuredContent as { trades: unknown[] }).trades).toHaveLength(2);

    const series = await client.callTool({ name: "get_chart_series", arguments: { range: "24h" } });
    expect(series.structuredContent).toMatchObject({ range: "24h", stepMs: 3_600_000, points: [{ nav: "7000" }, { nav: "7500.25" }] });
  });

  it("cannot read another identity's account, whichever tool it asks with", async () => {
    const { mcpUrl, otherToken } = await stack();
    const client = await agent(mcpUrl, otherToken);
    for (const name of ["get_nav", "get_portfolio", "get_performance", "get_trades", "get_chart_series"]) {
      const result = await client.callTool({ name, arguments: { accountId: ME } });
      expect(result.isError, name).toBe(true);
      expect((result.content as { text: string }[])[0]!.text, name).toContain("cross_account_access_denied");
    }
    // Its own state is answered, and lists nothing, because it connected nothing.
    const state = await client.callTool({ name: "get_account_state", arguments: {} });
    expect(state.structuredContent).toMatchObject({ address: SOMEONE_ELSE, accounts: [] });
  });

  it("stops working the moment the owner revokes the token", async () => {
    const { mcpUrl, token, tokenId, readOnlyTokenStore } = await stack();
    const client = await agent(mcpUrl, token);
    expect((await client.callTool({ name: "get_nav", arguments: {} })).isError).toBeFalsy();

    expect(await readOnlyTokenStore.revoke(ME, tokenId, new Date())).toBe(true);

    const afterRevoke = await client.callTool({ name: "get_nav", arguments: {} });
    expect(afterRevoke.isError).toBe(true);
    expect((afterRevoke.content as { text: string }[])[0]!.text).toContain("token_revoked");
  });

  it("is refused when it presents no token at all", async () => {
    const { mcpUrl } = await stack();
    const client = new Client({ name: "tokenless-agent", version: "0.0.0" });
    await client.connect(new StreamableHTTPClientTransport(mcpUrl));
    closers.push(() => client.close());
    const result = await client.callTool({ name: "get_nav", arguments: {} });
    expect(result.isError).toBe(true);
  });

  // An agent that bypasses this server and presents the same token
  // straight to the API must get no further: the token is the limit, not
  // the tool list in front of it.
  it("holds a token that changes nothing even against the API directly", async () => {
    const { apiOrigin, token } = await stack();
    const writes: [string, string, unknown][] = [
      ["POST", `/accounts/${ME}/binance-connection`, { apiKey: "k", apiSecret: "s" }],
      ["POST", `/accounts/${ME}/binance-sync`, {}],
      ["PATCH", `/accounts/${ME}/label`, { label: "renamed" }],
      ["DELETE", `/accounts/${ME}`, undefined],
      ["PUT", "/profile/settings", { privacyMode: true }],
      ["POST", "/api-tokens", { label: "a token minting a token" }],
    ];
    for (const [method, path, body] of writes) {
      const response = await fetch(`${apiOrigin}${path}`, {
        method,
        headers: body === undefined ? { authorization: `Bearer ${token}` } : { authorization: `Bearer ${token}`, "content-type": "application/json" },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      expect(response.status, `${method} ${path}`).toBe(401);
    }

    // The account is still there, unchanged, and still readable.
    const read = await fetch(`${apiOrigin}/mcp/account/state`, { headers: { authorization: `Bearer ${token}` } });
    expect(read.status).toBe(200);
    expect((await read.json()).accounts).toHaveLength(1);
  });
});
