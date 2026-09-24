// The MCP Streamable HTTP transport, on Node's own HTTP server.
//
// Stateless: a server and a transport are built per request and closed
// with it. There is nothing to keep between calls — every tool reads and
// the caller's token comes with each request — and a server with no
// session state cannot mix one identity's call up with another's.

import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { StreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/streamableHttp.js";
import { createMcpServer, type McpServerOptions } from "./server.js";

/** Where the protocol lives, per the MCP HTTP transport. */
export const MCP_PATH = "/mcp";

export interface HttpServerOptions extends McpServerOptions {
  /** Answers a liveness check without touching the API. */
  healthPath?: string;
}

async function handle(request: IncomingMessage, response: ServerResponse, options: HttpServerOptions): Promise<void> {
  const server = createMcpServer(options);
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
  return createServer((request, response) => {
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
