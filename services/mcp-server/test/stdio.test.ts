import { describe, expect, it } from "vitest";
import { serverOptions } from "../src/stdio.js";

// `StdioClientTransport` only ever talks to a spawned process, never to
// injected streams, so an end-to-end handshake over real stdio belongs
// in a heavier, process-spawning test rather than here. What is this
// file's own logic — turning the one required `token` into the
// `defaultToken` `createMcpServer` reads with, and nothing else about
// the caller's options — is what this tests directly.

describe("serverOptions", () => {
  it("becomes the McpServerOptions defaultToken — the one credential stdio ever reads with", () => {
    const token = `lvz_ro_${"a".repeat(64)}`;
    expect(serverOptions({ baseUrl: "http://127.0.0.1:4301", token })).toEqual({
      baseUrl: "http://127.0.0.1:4301",
      defaultToken: token,
    });
  });

  it("carries every other option through unchanged", () => {
    const fetchImpl = (() => Promise.reject(new Error("unused"))) as unknown as typeof globalThis.fetch;
    const mapped = serverOptions({ baseUrl: "http://127.0.0.1:4301", token: "lvz_ro_x", timeoutMs: 5_000, fetch: fetchImpl });
    expect(mapped.timeoutMs).toBe(5_000);
    expect(mapped.fetch).toBe(fetchImpl);
  });

  it("never leaves a bare `token` field behind for something to read by the wrong name", () => {
    const mapped = serverOptions({ baseUrl: "http://127.0.0.1:4301", token: "lvz_ro_x" });
    expect(Object.prototype.hasOwnProperty.call(mapped, "token")).toBe(false);
  });
});
