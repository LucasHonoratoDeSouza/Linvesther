// Runs the MCP server. Everything it needs comes from the environment;
// nothing it needs is a secret this process stores.
//
// Two transports, two trust models:
//
// - http (default): a per-call bearer token, always — see http.ts's own
//   doc comment for why LINVESTHER_READ_ONLY_TOKEN is never read here.
// - stdio (MCP_TRANSPORT=stdio): a direct, trusted parent<->child pipe
//   with no per-call credential of its own — LINVESTHER_READ_ONLY_TOKEN
//   is required, and is exactly what every call reads with.

import { createHttpServer, MCP_PATH } from "./http.js";
import { runStdioServer } from "./stdio.js";
import { DEFAULT_TIMEOUT_MS } from "./api.js";

const transport = process.env.MCP_TRANSPORT === "stdio" ? "stdio" : "http";
const baseUrl = process.env.LINVESTHER_API_URL ?? "http://127.0.0.1:4301";
const timeoutMs = process.env.LINVESTHER_API_TIMEOUT_MS ? Number(process.env.LINVESTHER_API_TIMEOUT_MS) : DEFAULT_TIMEOUT_MS;

if (transport === "stdio") {
  const token = process.env.LINVESTHER_READ_ONLY_TOKEN;
  if (!token) {
    console.error(
      "MCP_TRANSPORT=stdio needs LINVESTHER_READ_ONLY_TOKEN: stdio has no per-call credential, so this process has nothing to read with otherwise. Create one at /settings/api-tokens.",
    );
    process.exit(1);
  }
  await runStdioServer({ baseUrl, token, timeoutMs });
} else {
  const port = Number(process.env.MCP_PORT ?? 4302);
  // Bound to this machine by default: this server answers with one
  // identity's balances and trades, so reaching it from the network is a
  // deployment decision, never the default.
  const host = process.env.MCP_HOST ?? "127.0.0.1";
  const allowedHostnames = process.env.MCP_ALLOWED_HOSTS
    ? process.env.MCP_ALLOWED_HOSTS.split(",")
        .map((h) => h.trim())
        .filter(Boolean)
    : undefined;
  if (process.env.LINVESTHER_READ_ONLY_TOKEN) {
    // Not an error — a person who set this expecting personal, tokenless
    // use over HTTP needs to hear that it does nothing here, not
    // silently get the isolation this split exists to guarantee.
    console.warn(
      "LINVESTHER_READ_ONLY_TOKEN is set, but this is the http transport: it is never used here, on purpose (see http.ts). Every HTTP call must carry its own bearer token, or it is refused. For a single configured token, run with MCP_TRANSPORT=stdio instead.",
    );
  } else {
    console.warn("No caller will authenticate without its own token: every call must carry `Authorization: Bearer lvz_ro_…`, or it is refused.");
  }
  createHttpServer({ baseUrl, timeoutMs, allowedHostnames }).listen(port, host, () => {
    console.log(`mcp-server listening on http://${host}:${port}${MCP_PATH} (reading ${baseUrl})`);
  });
}
