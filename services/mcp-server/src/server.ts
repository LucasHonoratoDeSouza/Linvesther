// The MCP server: the six read-only tools, and the rule for deciding
// which token a call is made with.

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { ReadOnlyAccountApi, ReadOnlyApiError, ReadOnlyApiUnreachableError, type ReadOnlyApiOptions } from "./api.js";
import { READ_ONLY_TOOLS } from "./tools.js";

export const SERVER_NAME = "linvestherzk-account";
export const SERVER_VERSION = "0.1.0";

export interface McpServerOptions extends ReadOnlyApiOptions {
  /** The token to use when a call carries none of its own. Set when one
   * person runs this server for their own agent; left unset when the
   * server stands in front of several identities, each presenting its
   * own token. */
  defaultToken?: string;
}

/** What a client is told when a call carries no token at all, in the
 * words of the thing it has to do about it. */
const NO_TOKEN =
  "No Linvesther read-only token was presented. Send it as `Authorization: Bearer lvz_ro_…` on the MCP request, or configure LINVESTHER_READ_ONLY_TOKEN on this server. Create one at /settings/api-tokens.";

/** The token this call reads with: the one the caller presented, else
 * the one this server was configured with.
 *
 * A caller's own token always wins, so a server configured for one
 * person cannot quietly answer somebody else's call with that person's
 * credential. */
export function tokenForCall(headers: Record<string, unknown> | undefined, defaultToken: string | undefined): string | null {
  const header = headers?.authorization;
  const value = Array.isArray(header) ? header[0] : header;
  if (typeof value === "string") {
    const space = value.indexOf(" ");
    if (space > 0 && value.slice(0, space).toLowerCase() === "bearer") {
      const presented = value.slice(space + 1).trim();
      if (presented.length > 0) return presented;
    }
  }
  return defaultToken ?? null;
}

/** A refusal the caller can act on: which account, which credential,
 * which limit — never a stack trace and never the token. */
function failure(message: string): CallToolResult {
  return { content: [{ type: "text", text: message }], isError: true };
}

export function createMcpServer(options: McpServerOptions): McpServer {
  const api = new ReadOnlyAccountApi(options);
  const server = new McpServer(
    { name: SERVER_NAME, version: SERVER_VERSION },
    {
      instructions:
        "Reads one Linvesther identity's own account: connected accounts, current value, positions and profit, performance metrics, executed trades and the value/return series. Everything here is read-only — there is no tool to trade, transfer, connect an account or change a setting, and this server cannot do any of those things.",
    },
  );

  for (const tool of READ_ONLY_TOOLS) {
    server.registerTool(
      tool.name,
      {
        title: tool.title,
        description: tool.description,
        inputSchema: tool.inputSchema,
        outputSchema: tool.outputSchema,
        annotations: {
          title: tool.title,
          readOnlyHint: true,
          destructiveHint: false,
          idempotentHint: true,
          // Reaches the Linvesther API over the network.
          openWorldHint: true,
        },
      },
      async (input, extra) => {
        const token = tokenForCall(extra.requestInfo?.headers as Record<string, unknown> | undefined, options.defaultToken);
        if (!token) return failure(NO_TOKEN);
        try {
          const answer = (await tool.call(api, token, (input ?? {}) as Record<string, never>)) as Record<string, unknown>;
          return {
            // Both forms, since a client may read either: the structured
            // answer is what the output schema describes, and the text is
            // the same thing, not a summary of it.
            structuredContent: answer,
            content: [{ type: "text", text: JSON.stringify(answer, null, 2) }],
          };
        } catch (error) {
          if (error instanceof ReadOnlyApiError) {
            return failure(`Linvesther refused this read (${error.status}): ${error.message}`);
          }
          if (error instanceof ReadOnlyApiUnreachableError) {
            return failure(`The Linvesther API could not be reached: ${error.message}`);
          }
          throw error;
        }
      },
    );
  }

  return server;
}
