// Runs the MCP server. Everything it needs comes from the environment;
// nothing it needs is a secret this process stores.

import { createHttpServer, MCP_PATH } from "./http.js";
import { DEFAULT_TIMEOUT_MS } from "./api.js";

const port = Number(process.env.MCP_PORT ?? 4302);
// Bound to this machine by default: this server answers with one
// identity's balances and trades, so reaching it from the network is a
// deployment decision, never the default.
const host = process.env.MCP_HOST ?? "127.0.0.1";
const baseUrl = process.env.LINVESTHER_API_URL ?? "http://127.0.0.1:4301";
const defaultToken = process.env.LINVESTHER_READ_ONLY_TOKEN;
const timeoutMs = process.env.LINVESTHER_API_TIMEOUT_MS ? Number(process.env.LINVESTHER_API_TIMEOUT_MS) : DEFAULT_TIMEOUT_MS;

if (!defaultToken) {
  console.warn(
    "LINVESTHER_READ_ONLY_TOKEN is not set: every call must carry its own token as `Authorization: Bearer lvz_ro_…`, or it will be refused",
  );
}

createHttpServer({ baseUrl, defaultToken, timeoutMs }).listen(port, host, () => {
  console.log(`mcp-server listening on http://${host}:${port}${MCP_PATH} (reading ${baseUrl})`);
});
