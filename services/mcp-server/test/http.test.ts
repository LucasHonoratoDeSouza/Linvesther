import type { AddressInfo } from "node:net";
import type { Server } from "node:http";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { afterEach, describe, expect, it } from "vitest";
import { createHttpServer, MCP_PATH } from "../src/http.js";

// The HTTP transport, driven by a real MCP client over a real socket
// against a stub standing in for the Linvesther API. What the in-memory
// suite cannot cover lives here: the token each call carries comes from
// that call's own HTTP request.

const ADDRESS = "0x1111111111111111111111111111111111111111";
const NAV = { accountId: ADDRESS, nav: "1234.50", currency: "USDT", assets: [], excludedOutOfScopeAssets: [] };

let running: Server | undefined;

afterEach(async () => {
  if (running) await new Promise<void>((resolve) => running!.close(() => resolve()));
  running = undefined;
});

/** Records the token each read arrived with, and answers every read. */
function stubApi() {
  const tokens: (string | null)[] = [];
  const fetchImpl = (async (input: string | URL | Request, init?: RequestInit) => {
    const headers = new Headers(init?.headers);
    tokens.push(headers.get("authorization"));
    return new Response(JSON.stringify(NAV), { status: 200, headers: { "content-type": "application/json" } });
  }) as typeof globalThis.fetch;
  return { tokens, fetchImpl };
}

async function listening(defaultToken?: string) {
  const { tokens, fetchImpl } = stubApi();
  const server = createHttpServer({ baseUrl: "http://api.invalid", fetch: fetchImpl, defaultToken });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  running = server;
  const { port } = server.address() as AddressInfo;
  return { tokens, url: new URL(`http://127.0.0.1:${port}${MCP_PATH}`), healthUrl: `http://127.0.0.1:${port}/healthz` };
}

async function connect(url: URL, token?: string) {
  const client = new Client({ name: "http-test-client", version: "0.0.0" });
  await client.connect(
    new StreamableHTTPClientTransport(url, token ? { requestInit: { headers: { authorization: `Bearer ${token}` } } } : undefined),
  );
  return client;
}

describe("the MCP server over HTTP", () => {
  it("completes the protocol handshake and lists its tools", async () => {
    const { url } = await listening(`lvz_ro_${"a".repeat(64)}`);
    const client = await connect(url);
    const { tools } = await client.listTools();
    expect(tools).toHaveLength(6);
    await client.close();
  });

  it("reads with the token the calling client presented on its own request", async () => {
    const presented = `lvz_ro_${"c".repeat(64)}`;
    const { tokens, url } = await listening(`lvz_ro_${"a".repeat(64)}`);
    const client = await connect(url, presented);
    const result = await client.callTool({ name: "get_nav", arguments: {} });
    expect(result.structuredContent).toEqual(NAV);
    expect(tokens).toEqual([`Bearer ${presented}`]);
    await client.close();
  });

  it("falls back to the configured token when the client presents none", async () => {
    const configured = `lvz_ro_${"a".repeat(64)}`;
    const { tokens, url } = await listening(configured);
    const client = await connect(url);
    await client.callTool({ name: "get_nav", arguments: {} });
    expect(tokens).toEqual([`Bearer ${configured}`]);
    await client.close();
  });

  it("refuses a call it has no token for, and reads nothing", async () => {
    const { tokens, url } = await listening();
    const client = await connect(url);
    const result = await client.callTool({ name: "get_nav", arguments: {} });
    expect(result.isError).toBe(true);
    expect(tokens).toEqual([]);
    await client.close();
  });

  it("answers a liveness check without reading anything", async () => {
    const { tokens, healthUrl } = await listening();
    const response = await fetch(healthUrl);
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ ok: true });
    expect(tokens).toEqual([]);
  });

  it("serves the protocol at one path and nothing anywhere else", async () => {
    const { url } = await listening();
    const response = await fetch(new URL("/accounts", url), { method: "POST", body: "{}" });
    expect(response.status).toBe(404);
  });
});
