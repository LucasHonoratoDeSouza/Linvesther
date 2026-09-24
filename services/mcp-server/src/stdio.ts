// The stdio transport: a direct, trusted parent<->child pipe — the
// classic way a personal MCP client (Claude Desktop, an IDE, a CLI)
// runs a server for its own use. There is no per-call header on stdio —
// no concept of one — so the only place a token can ever come from is
// whatever this process was started with. That is exactly the trust
// model stdio already assumes: whoever can spawn this process and read
// its stdout already controls everything the token would let them read.
//
// This is the only place `LINVESTHER_READ_ONLY_TOKEN` is honored. The
// HTTP transport (`http.ts`) never sees it — see that file's own doc
// comment for why a network transport falling back to a configured
// token is exactly the bug this split exists to make structurally
// impossible, not just avoided by convention.

import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { createMcpServer, type McpServerOptions } from "./server.js";
import type { ReadOnlyApiOptions } from "./api.js";

export interface StdioServerOptions extends ReadOnlyApiOptions {
  /** Required, not defaulted: stdio has no per-call credential, so every
   * read this process makes reads with this one token. A caller with no
   * token to configure has nothing to run stdio mode with. */
  token: string;
}

/** The `McpServerOptions` this transport's one, required token becomes —
 * exported so the mapping itself is directly testable without spawning
 * a real child process over real stdio, which is what actually driving
 * `runStdioServer` end to end would need (`StdioClientTransport` only
 * ever talks to a spawned process, never to injected streams). */
export function serverOptions(options: StdioServerOptions): McpServerOptions {
  const { token, ...rest } = options;
  return { ...rest, defaultToken: token };
}

/** Connects the MCP server to this process's own stdin/stdout and
 * returns once connected — the process stays alive on its own from
 * there, the same way every stdio MCP server does (an open stdin keeps
 * the event loop running). */
export async function runStdioServer(options: StdioServerOptions): Promise<void> {
  const server = createMcpServer(serverOptions(options));
  await server.connect(new StdioServerTransport());
}
