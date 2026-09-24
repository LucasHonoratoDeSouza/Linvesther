// The MCP Streamable HTTP transport, on Node's own HTTP server.
//
// Stateless: a server and a transport are built per request and closed
// with it. There is nothing to keep between calls — every tool reads and
// the caller's token comes with each request — and a server with no
// session state cannot mix one identity's call up with another's.
//
// Never honors a configured default token. `HttpServerOptions` extends
// `ReadOnlyApiOptions`, not `McpServerOptions` — the type this file
// builds a server from structurally cannot carry a `defaultToken` at
// all, so there is no field here for one to leak through even if a
// caller's own object happened to have one (see `handle`, below, which
// still names each field explicitly rather than spreading `options`, as
// a second, independent guarantee of the same thing). A request that
// carries no bearer token, or a malformed one, is answered by the MCP
// server itself with a real refusal — never silently answered as
// somebody else's account. Personal, local use with one configured
// token is `stdio.ts`'s own transport, which has no such hole because it
// has no per-call header to fall back from in the first place.

import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { StreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/streamableHttp.js";
import { createMcpServer } from "./server.js";
import type { ReadOnlyApiOptions } from "./api.js";

/** Where the protocol lives, per the MCP HTTP transport. */
export const MCP_PATH = "/mcp";

/** Loopback aliases, in the form a `Host`/`Origin` header carries them
 * (bare hostname, no port, no scheme). Right for the default bind
 * address (127.0.0.1) and for any client on the same machine; a
 * deliberate deployment behind a real hostname must set its own list. */
export const DEFAULT_ALLOWED_HOSTNAMES = ["localhost", "127.0.0.1", "::1", "[::1]"];

export interface HttpServerOptions extends ReadOnlyApiOptions {
  /** Answers a liveness check without touching the API. */
  healthPath?: string;
  /** DNS rebinding protection: the hostname a `Host` header carries —
   * and an `Origin` header's, when a request sends one — must be one of
   * these, or the request never reaches the MCP transport. Defaults to
   * `DEFAULT_ALLOWED_HOSTNAMES`. */
  allowedHostnames?: string[];
}

/** The hostname a `Host` or `Origin` header value names, lowercased and
 * with its port (and, for `Origin`, its scheme) stripped — `null` for
 * anything that is not a well-formed one of either. An IPv6 literal
 * keeps its brackets (`Host: [::1]:4302` and `Origin: http://[::1]:4302`
 * both name `[::1]`), since that is also how `DEFAULT_ALLOWED_HOSTNAMES`
 * spells it. */
export function headerHostname(value: string | undefined): string | null {
  if (!value) return null;
  const schemeEnd = value.indexOf("://");
  const authority = schemeEnd === -1 ? value : value.slice(schemeEnd + 3);
  if (authority.length === 0) return null;
  if (authority.startsWith("[")) {
    const closing = authority.indexOf("]");
    return closing === -1 ? null : authority.slice(0, closing + 1).toLowerCase();
  }
  const colon = authority.lastIndexOf(":");
  const hostname = colon === -1 ? authority : authority.slice(0, colon);
  return hostname.length > 0 ? hostname.toLowerCase() : null;
}

/** Whether this request's `Host` — and its `Origin`, when it sends one —
 * name a hostname this server accepts. A request with no `Host` header
 * at all is refused (HTTP/1.1 requires one); a request with no `Origin`
 * is not (a non-browser MCP client typically sends none, and absence is
 * not what this defends against — a forged, rebound *Host* naming an
 * attacker's own domain is). */
export function isAllowedRequest(headers: { host?: string; origin?: string }, allowedHostnames: readonly string[]): boolean {
  const host = headerHostname(headers.host);
  if (host === null || !allowedHostnames.includes(host)) return false;
  if (headers.origin !== undefined) {
    const origin = headerHostname(headers.origin);
    if (origin === null || !allowedHostnames.includes(origin)) return false;
  }
  return true;
}

async function handle(request: IncomingMessage, response: ServerResponse, options: HttpServerOptions): Promise<void> {
  // Named explicitly, not `createMcpServer(options)`: this is the second
  // of the two guarantees described in this file's own doc comment — a
  // stray `defaultToken` on `options` at runtime (a type assertion
  // upstream, a future refactor) still could not reach the server this
  // builds, because nothing here ever reads that field off `options` to
  // begin with.
  const server = createMcpServer({ baseUrl: options.baseUrl, timeoutMs: options.timeoutMs, fetch: options.fetch });
  const transport = new StreamableHTTPServerTransport({
    // Stateless: no session id is issued and none is required.
    sessionIdGenerator: undefined,
    // One JSON answer per call, rather than a stream — every tool here
    // returns once and has nothing to stream.
    enableJsonResponse: true,
  });
  response.on("close", () => {
    void transport.close();
    void server.close();
  });
  await server.connect(transport);
  await transport.handleRequest(request, response);
}

export function createHttpServer(options: HttpServerOptions): Server {
  const healthPath = options.healthPath ?? "/healthz";
  const allowedHostnames = options.allowedHostnames ?? DEFAULT_ALLOWED_HOSTNAMES;
  return createServer((request, response) => {
    if (!isAllowedRequest({ host: request.headers.host, origin: request.headers.origin }, allowedHostnames)) {
      response.writeHead(400, { "content-type": "application/json" }).end(JSON.stringify({ error: "host_not_allowed" }));
      return;
    }
    const path = (request.url ?? "/").split("?")[0];
    if (path === healthPath) {
      response.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify({ ok: true }));
      return;
    }
    if (path !== MCP_PATH) {
      response.writeHead(404, { "content-type": "application/json" }).end(JSON.stringify({ error: "not_found" }));
      return;
    }
    handle(request, response, options).catch((error: unknown) => {
      console.error("mcp request failed", error);
      if (!response.headersSent) {
        response.writeHead(500, { "content-type": "application/json" });
      }
      if (!response.writableEnded) {
        response.end(JSON.stringify({ jsonrpc: "2.0", error: { code: -32603, message: "internal error" }, id: null }));
      }
    });
  });
}
