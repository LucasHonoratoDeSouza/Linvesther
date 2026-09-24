import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { describe, expect, it } from "vitest";
import { createMcpServer, tokenForCall, SERVER_NAME } from "../src/server.js";
import { READ_ONLY_TOOLS } from "../src/tools.js";

// The tools are driven through a real MCP client over an in-memory
// transport pair, against a stub standing in for the Linvesther API —
// so what is asserted is what a client actually sees (schemas,
// annotations, structured answers, errors), not the shape of an
// internal object.

const TOKEN = `lvz_ro_${"a".repeat(64)}`;
const ADDRESS = "0x1111111111111111111111111111111111111111";

const STATE = {
  address: ADDRESS,
  scope: "read:account",
  accounts: [
    {
      accountId: ADDRESS,
      broker: "binance",
      label: "Main",
      connectedAtMs: 1_700_000_000_000,
      status: "active",
      connection: { connected: true, status: "active", lastSyncedAt: "2026-09-01T00:00:00Z", tradeCount: 7, flowCount: 2 },
      connectionUnavailable: null,
      balance: { nav: "1234.50", currency: "USDT" },
    },
  ],
  balance: { total: "1234.50", currency: "USDT" },
  balanceUnavailable: null,
};

const PORTFOLIO = {
  accountId: ADDRESS,
  positions: [{ asset: "BTC", quantity: "0.5", averageCost: "40000", markPrice: "60000", realized: "10", unrealized: "10000" }],
  realized: "10",
  unrealized: "10000",
  fees: "1.5",
  sinceMs: 1_700_000_000_000,
};

const NAV = {
  accountId: ADDRESS,
  nav: "1234.50",
  currency: "USDT",
  assets: [{ asset: "BTC", quantity: "0.5", price: "60000", value: "30000" }],
  excludedOutOfScopeAssets: ["SOMECOIN"],
};

const PERFORMANCE = {
  accountId: ADDRESS,
  cagr: null,
  cagrUnavailable: "a year has not passed since the connection",
  sharpe: "1.25",
  sharpeUnavailable: null,
  sortino: null,
  sortinoUnavailable: "not enough days below the mean yet",
  maxDrawdown: "0.08",
  winRate: { closedRoundTrips: 4, wins: 3, winRate: "0.75" },
  sinceMs: 1_700_000_000_000,
};

const TRADES = {
  accountId: ADDRESS,
  trades: [
    { symbol: "BTCUSDT", tradeId: 1, orderId: 10, price: "40000", quantity: "0.1", commission: "0.4", commissionAsset: "USDT", timeMs: 1_700_000_000_000, side: "buy" },
  ],
  nextCursor: "1700000000000:1:BTCUSDT",
  sinceMs: 1_699_000_000_000,
};

const SERIES = { accountId: ADDRESS, range: "7d", points: [{ timeMs: 1, nav: "100", index: "1" }], stepMs: 3_600_000, sinceMs: 1 };

const BY_PATH: Record<string, unknown> = {
  "/mcp/account/state": STATE,
  "/mcp/account/portfolio": PORTFOLIO,
  "/mcp/account/nav": NAV,
  "/mcp/account/performance": PERFORMANCE,
  "/mcp/account/trades": TRADES,
  "/mcp/account/series": SERIES,
};

/** A stand-in for `apps/api`: records what was asked and answers with a
 * fixed body, or with whatever refusal a test configured. */
function stubApi(refusal?: { status: number; body: Record<string, unknown> }) {
  const requests: { url: URL; authorization: string | null }[] = [];
  const fetchImpl = (async (input: string | URL | Request, init?: RequestInit) => {
    const url = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url);
    const headers = new Headers(init?.headers);
    requests.push({ url, authorization: headers.get("authorization") });
    if (refusal) {
      return new Response(JSON.stringify(refusal.body), { status: refusal.status, headers: { "content-type": "application/json" } });
    }
    return new Response(JSON.stringify(BY_PATH[url.pathname] ?? {}), { status: 200, headers: { "content-type": "application/json" } });
  }) as typeof globalThis.fetch;
  return { requests, fetchImpl };
}

async function connected(options: { refusal?: { status: number; body: Record<string, unknown> }; defaultToken?: string | undefined } = {}) {
  const { requests, fetchImpl } = stubApi(options.refusal);
  const server = createMcpServer({
    baseUrl: "http://api.invalid",
    fetch: fetchImpl,
    defaultToken: "defaultToken" in options ? options.defaultToken : TOKEN,
  });
  const client = new Client({ name: "test-client", version: "0.0.0" });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);
  return { client, server, requests, close: async () => { await client.close(); await server.close(); } };
}

describe("what this server offers", () => {
  it("is exactly six tools, and every one of them only reads", async () => {
    const { client, close } = await connected();
    const { tools } = await client.listTools();
    expect(tools.map((t) => t.name).sort()).toEqual([
      "get_account_state",
      "get_chart_series",
      "get_nav",
      "get_performance",
      "get_portfolio",
      "get_trades",
    ]);
    for (const tool of tools) {
      expect(tool.annotations?.readOnlyHint, tool.name).toBe(true);
      expect(tool.annotations?.destructiveHint, tool.name).toBe(false);
      expect(tool.description, tool.name).toContain("Reads only");
    }
    await close();
  });

  // Not "no write tool is registered today": a name that trades,
  // transfers, connects or changes anything must never appear here.
  it("offers nothing that acts", async () => {
    const { client, close } = await connected();
    const { tools } = await client.listTools();
    for (const tool of tools) {
      expect(tool.name, tool.name).toMatch(/^get_/);
      for (const verb of ["place", "cancel", "order", "trade_", "withdraw", "transfer", "connect", "disconnect", "rename", "sync", "set_", "update", "delete", "revoke"]) {
        expect(tool.name.includes(verb), `${tool.name} contains ${verb}`).toBe(false);
      }
    }
    await close();
  });

  it("publishes a JSON Schema for what each tool takes and returns", async () => {
    const { client, close } = await connected();
    const { tools } = await client.listTools();
    for (const tool of tools) {
      expect(tool.inputSchema, tool.name).toMatchObject({ type: "object" });
      expect(tool.outputSchema, tool.name).toMatchObject({ type: "object", properties: expect.any(Object) });
    }
    // The one tool that takes a filter says what each filter means.
    const trades = tools.find((t) => t.name === "get_trades");
    expect(Object.keys((trades?.inputSchema as { properties: Record<string, unknown> }).properties).sort()).toEqual([
      "accountId",
      "cursor",
      "limit",
      "since",
      "symbol",
      "until",
    ]);
    await close();
  });

  it("names itself, so a client can tell which server it is talking to", async () => {
    const { client, close } = await connected();
    expect(client.getServerVersion()?.name).toBe(SERVER_NAME);
    await close();
  });
});

describe("calling a tool", () => {
  it("answers each read with the figures the API returned, structured and as text", async () => {
    const { client, close } = await connected();
    const expected: [string, Record<string, unknown>, unknown][] = [
      ["get_account_state", {}, STATE],
      ["get_portfolio", {}, PORTFOLIO],
      ["get_nav", {}, NAV],
      ["get_performance", {}, PERFORMANCE],
      ["get_trades", {}, TRADES],
      ["get_chart_series", { range: "7d" }, SERIES],
    ];
    for (const [name, args, body] of expected) {
      const result = await client.callTool({ name, arguments: args });
      expect(result.isError, name).toBeFalsy();
      expect(result.structuredContent, name).toEqual(body);
      // The text is the same answer, not a summary of it.
      expect(JSON.parse((result.content as { text: string }[])[0]!.text), name).toEqual(body);
    }
    await close();
  });

  it("reads each tool's own route, and carries the token on every one", async () => {
    const { client, requests, close } = await connected();
    for (const name of READ_ONLY_TOOLS.map((t) => t.name)) await client.callTool({ name, arguments: {} });
    expect(requests.map((r) => r.url.pathname).sort()).toEqual(Object.keys(BY_PATH).sort());
    expect(requests.every((r) => r.authorization === `Bearer ${TOKEN}`)).toBe(true);
    await close();
  });

  it("passes the filters it was given, and leaves out the ones it was not", async () => {
    const { client, requests, close } = await connected();
    await client.callTool({ name: "get_trades", arguments: { accountId: `${ADDRESS}_swing`, symbol: "BTCUSDT", since: 1000, until: 2000, limit: 50 } });
    const query = requests[0]!.url.searchParams;
    expect(Object.fromEntries(query)).toEqual({ accountId: `${ADDRESS}_swing`, symbol: "BTCUSDT", since: "1000", until: "2000", limit: "50" });
    expect(query.has("cursor")).toBe(false);
    await close();
  });

  it("refuses an argument the schema does not allow, before any read happens", async () => {
    const { client, requests, close } = await connected();
    const refused: [string, Record<string, unknown>][] = [
      ["get_trades", { limit: 5000 }],
      ["get_trades", { limit: 0 }],
      ["get_trades", { since: -1 }],
      ["get_trades", { symbol: "x".repeat(41) }],
      ["get_chart_series", { range: "3d" }],
      ["get_nav", { accountId: "a".repeat(81) }],
    ];
    for (const [name, args] of refused) {
      const result = await client.callTool({ name, arguments: args });
      expect(result.isError, `${name} ${JSON.stringify(args)}`).toBe(true);
      expect((result.content as { text: string }[])[0]!.text).toContain("validation");
    }
    expect(requests, "nothing should have been read").toEqual([]);
    await close();
  });

  it("reports a refusal from the API as a failed call, with the reason", async () => {
    const { client, close } = await connected({ refusal: { status: 403, body: { error: "cross_account_access_denied" } } });
    const result = await client.callTool({ name: "get_nav", arguments: { accountId: "0x2222222222222222222222222222222222222222" } });
    expect(result.isError).toBe(true);
    expect((result.content as { text: string }[])[0]!.text).toContain("cross_account_access_denied");
    await close();
  });

  it("says a metric is unavailable rather than making one up", async () => {
    const { client, close } = await connected();
    const result = await client.callTool({ name: "get_performance", arguments: {} });
    const answer = result.structuredContent as typeof PERFORMANCE;
    expect(answer.cagr).toBeNull();
    expect(answer.cagrUnavailable).toBe("a year has not passed since the connection");
    await close();
  });

  it("reports a revoked token as the refusal it is", async () => {
    const { client, close } = await connected({ refusal: { status: 401, body: { error: "token_revoked" } } });
    const result = await client.callTool({ name: "get_nav", arguments: {} });
    expect(result.isError).toBe(true);
    expect((result.content as { text: string }[])[0]!.text).toContain("token_revoked");
    await close();
  });

  it("says what to do when no token was presented at all", async () => {
    const { client, requests, close } = await connected({ defaultToken: undefined });
    const result = await client.callTool({ name: "get_nav", arguments: {} });
    expect(result.isError).toBe(true);
    expect((result.content as { text: string }[])[0]!.text).toContain("Authorization: Bearer");
    expect(requests).toEqual([]);
    await close();
  });
});

describe("which token a call reads with", () => {
  const configured = `lvz_ro_${"b".repeat(64)}`;

  it("is the caller's own whenever they present one", () => {
    expect(tokenForCall({ authorization: `Bearer ${TOKEN}` }, configured)).toBe(TOKEN);
    expect(tokenForCall({ authorization: `bearer ${TOKEN}` }, configured)).toBe(TOKEN);
  });

  it("falls back to the configured one only when the caller presents none", () => {
    expect(tokenForCall(undefined, configured)).toBe(configured);
    expect(tokenForCall({}, configured)).toBe(configured);
    expect(tokenForCall({ authorization: "Basic abc" }, configured)).toBe(configured);
    expect(tokenForCall({ authorization: "Bearer " }, configured)).toBe(configured);
  });

  it("is nothing at all when neither exists", () => {
    expect(tokenForCall(undefined, undefined)).toBeNull();
    expect(tokenForCall({ authorization: "Bearer" }, undefined)).toBeNull();
  });
});
