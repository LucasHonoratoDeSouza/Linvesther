import { request as httpRequest, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { afterEach, describe, expect, it } from "vitest";
import { createHttpServer, DEFAULT_ALLOWED_HOSTNAMES, headerHostname, isAllowedRequest, MCP_PATH, type HttpServerOptions } from "../src/http.js";

// The HTTP transport, driven by a real MCP client over a real socket
// against a stub standing in for the Linvesther API. What the in-memory
// suite cannot cover lives here: the token each call carries comes from
// that call's own HTTP request, and this transport must never fall back
// to a configured one — see http.ts's own doc comment for why.

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

async function listening(overrides: Partial<Omit<HttpServerOptions, "baseUrl" | "fetch">> = {}) {
  const { tokens, fetchImpl } = stubApi();
  const server = createHttpServer({ baseUrl: "http://api.invalid", fetch: fetchImpl, ...overrides });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  running = server;
  const { port } = server.address() as AddressInfo;
  return { tokens, port, url: new URL(`http://127.0.0.1:${port}${MCP_PATH}`), healthUrl: `http://127.0.0.1:${port}/healthz` };
}

async function connect(url: URL, token?: string) {
  const client = new Client({ name: "http-test-client", version: "0.0.0" });
  await client.connect(
    new StreamableHTTPClientTransport(url, token ? { requestInit: { headers: { authorization: `Bearer ${token}` } } } : undefined),
  );
  return client;
}

/** A raw HTTP request with an arbitrary `Host` header — `fetch()` cannot
 * do this (the Fetch spec forbids setting `Host` through it, and it is
 * always derived from the URL), but a real DNS-rebinding attack does not
 * need to: the browser sets `Host` from whatever domain the page's URL
 * actually named, which for a rebound request is the attacker's own
 * domain, not "localhost". `http.request` is used here only to produce
 * that same header on the wire for the test — not to exercise a real
 * client library. */
function rawRequest(port: number, path: string, headers: Record<string, string>): Promise<{ status: number; body: string }> {
  return new Promise((resolve, reject) => {
    const req = httpRequest({ host: "127.0.0.1", port, path, method: "GET", headers }, (res) => {
      let body = "";
      res.on("data", (chunk: Buffer) => (body += chunk.toString()));
      res.on("end", () => resolve({ status: res.statusCode ?? 0, body }));
    });
    req.on("error", reject);
    req.end();
  });
}

describe("the MCP server over HTTP", () => {
  it("completes the protocol handshake and lists its tools", async () => {
    const { url } = await listening();
    const client = await connect(url);
    const { tools } = await client.listTools();
    expect(tools).toHaveLength(6);
    await client.close();
  });

  it("reads with the token the calling client presented on its own request", async () => {
    const presented = `lvz_ro_${"c".repeat(64)}`;
    const { tokens, url } = await listening();
    const client = await connect(url, presented);
    const result = await client.callTool({ name: "get_nav", arguments: {} });
    expect(result.structuredContent).toEqual(NAV);
    expect(tokens).toEqual([`Bearer ${presented}`]);
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

  // The bug this whole split exists to make impossible: a network
  // request that carries no credential must be refused, in the words a
  // caller can act on, never silently answered — there being nothing on
  // HttpServerOptions for a default token to hide behind is the
  // structural half of that guarantee; this is the runtime half.
  it("refuses a tokenless call with a real refusal, never a fabricated success", async () => {
    const { tokens, url } = await listening();
    const client = await connect(url);
    const result = await client.callTool({ name: "get_nav", arguments: {} });
    expect(result.isError).toBe(true);
    expect((result.content as { text: string }[])[0]?.text).toContain("No Linvesther read-only token");
    expect(tokens).toEqual([]);
    await client.close();
  });

  // The exact scenario the finding named: LINVESTHER_READ_ONLY_TOKEN
  // configured, a caller presents no token of its own, over HTTP — must
  // be refused, never silently answered with the configured identity's
  // account. HttpServerOptions carries no `defaultToken` field at all,
  // so this cannot be expressed through the type system; the object
  // below forces one on at runtime (the shape a stray upstream cast, or
  // a future refactor that widens the type again, could produce) to
  // prove the second, independent guarantee: `handle()` in http.ts never
  // reads that field off `options` even when it is present.
  it("still refuses a tokenless call even when something that looks like a configured default token is on the options object", async () => {
    const { tokens, fetchImpl } = stubApi();
    const smuggled = { baseUrl: "http://api.invalid", fetch: fetchImpl, defaultToken: `lvz_ro_${"d".repeat(64)}` };
    const server = createHttpServer(smuggled as unknown as HttpServerOptions);
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    running = server;
    const { port } = server.address() as AddressInfo;
    const client = await connect(new URL(`http://127.0.0.1:${port}${MCP_PATH}`));
    const result = await client.callTool({ name: "get_nav", arguments: {} });
    expect(result.isError).toBe(true);
    expect((result.content as { text: string }[])[0]?.text).toContain("No Linvesther read-only token");
    expect(tokens, "the smuggled default token must never have been sent to the API").toEqual([]);
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

describe("DNS rebinding protection", () => {
  it("accepts the loopback Host a real client actually sends", async () => {
    const { port, tokens } = await listening();
    const response = await rawRequest(port, "/healthz", { host: "127.0.0.1" });
    expect(response.status).toBe(200);
    expect(tokens).toEqual([]);
  });

  // The attack this defends against: a page whose URL names a domain
  // the attacker controls, which resolves to 127.0.0.1 only after an
  // initial check passes. The browser's own Host header on that request
  // names the attacker's domain — never "localhost" — so this must
  // refuse it before the MCP transport, and the token check inside it,
  // ever run.
  it("refuses a request whose Host names a domain outside the allowlist, before any read happens", async () => {
    const { port, tokens } = await listening();
    const response = await rawRequest(port, MCP_PATH, {
      host: "rebound.attacker.example",
      authorization: `Bearer lvz_ro_${"a".repeat(64)}`,
      "content-type": "application/json",
      accept: "application/json, text/event-stream",
    });
    expect(response.status).toBe(400);
    expect(JSON.parse(response.body)).toEqual({ error: "host_not_allowed" });
    expect(tokens).toEqual([]);
  });

  it("refuses a spoofed Host even against the health check, which otherwise reveals nothing", async () => {
    const { port } = await listening();
    const response = await rawRequest(port, "/healthz", { host: "rebound.attacker.example" });
    expect(response.status).toBe(400);
  });

  it("refuses an Origin outside the allowlist even when the Host is a real one", async () => {
    const { port } = await listening();
    const response = await rawRequest(port, "/healthz", { host: "127.0.0.1", origin: "http://attacker.example" });
    expect(response.status).toBe(400);
  });

  it("does not refuse a request with no Origin at all — a non-browser MCP client typically sends none", async () => {
    const { port } = await listening();
    const response = await rawRequest(port, "/healthz", { host: "127.0.0.1" });
    expect(response.status).toBe(200);
  });

  it("widens to an operator-configured hostname, and nothing beyond it", async () => {
    const { port } = await listening({ allowedHostnames: ["mcp.internal.example"] });
    const allowed = await rawRequest(port, "/healthz", { host: "mcp.internal.example" });
    expect(allowed.status).toBe(200);
    const stillRefused = await rawRequest(port, "/healthz", { host: "127.0.0.1" });
    expect(stillRefused.status).toBe(400);
  });
});

describe("headerHostname", () => {
  it("strips the port from a bare Host value", () => {
    expect(headerHostname("127.0.0.1:4302")).toBe("127.0.0.1");
    expect(headerHostname("localhost:4302")).toBe("localhost");
    expect(headerHostname("localhost")).toBe("localhost");
  });

  it("strips the scheme and the port from an Origin value", () => {
    expect(headerHostname("http://127.0.0.1:4302")).toBe("127.0.0.1");
    expect(headerHostname("https://example.com")).toBe("example.com");
  });

  it("keeps an IPv6 literal's brackets, the same way the default allowlist spells it", () => {
    expect(headerHostname("[::1]:4302")).toBe("[::1]");
    expect(headerHostname("http://[::1]:4302")).toBe("[::1]");
  });

  it("lowercases, since a hostname is case-insensitive", () => {
    expect(headerHostname("LOCALHOST:4302")).toBe("localhost");
  });

  it("is null for nothing, and for a value with nothing left after stripping", () => {
    expect(headerHostname(undefined)).toBeNull();
    expect(headerHostname("")).toBeNull();
    expect(headerHostname(":4302")).toBeNull();
  });
});

describe("isAllowedRequest", () => {
  it("accepts every default loopback alias", () => {
    for (const host of DEFAULT_ALLOWED_HOSTNAMES) {
      expect(isAllowedRequest({ host: `${host}:4302` }, DEFAULT_ALLOWED_HOSTNAMES)).toBe(true);
    }
  });

  it("refuses a missing Host outright — HTTP/1.1 requires one", () => {
    expect(isAllowedRequest({}, DEFAULT_ALLOWED_HOSTNAMES)).toBe(false);
  });

  it("checks Host and Origin independently: either one outside the list refuses the request", () => {
    expect(isAllowedRequest({ host: "127.0.0.1", origin: "http://evil.example" }, DEFAULT_ALLOWED_HOSTNAMES)).toBe(false);
    expect(isAllowedRequest({ host: "evil.example", origin: "http://127.0.0.1" }, DEFAULT_ALLOWED_HOSTNAMES)).toBe(false);
    expect(isAllowedRequest({ host: "127.0.0.1", origin: "http://127.0.0.1" }, DEFAULT_ALLOWED_HOSTNAMES)).toBe(true);
  });
});
