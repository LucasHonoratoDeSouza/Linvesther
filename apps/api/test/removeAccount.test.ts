import { chmodSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { buildApp } from "../src/app.js";
import { MemorySessionStore } from "../src/auth/sessionStore.js";

const ME = "0x1111111111111111111111111111111111111111" as const;
const SOMEONE_ELSE = "0x2222222222222222222222222222222222222222" as const;

/** A stand-in for the worker binary that logs the subcommand it was asked to run. */
function fakeWorker(script: string) {
  const dir = mkdtempSync(join(tmpdir(), "remove-worker-"));
  const path = join(dir, "worker.sh");
  const log = join(dir, "calls.log");
  writeFileSync(log, "");
  writeFileSync(path, `#!/bin/sh\necho "$1" >> ${log}\ncat > /dev/null\n${script}\n`);
  chmodSync(path, 0o755);
  return { path, calls: () => (readFileSync(log, "utf8").trim() ? readFileSync(log, "utf8").trim().split("\n") : []) };
}

async function remove(script: string, accountId: string, signedIn = true) {
  const sessionStore = new MemorySessionStore();
  const worker = fakeWorker(script);
  const app = buildApp({ domain: "localhost", sessionStore, binanceWorkerBinaryPath: worker.path });
  await app.ready();
  const cookies = signedIn ? { sid: (await sessionStore.create(ME)).id } : undefined;
  const response = await app.inject({ method: "DELETE", url: `/accounts/${accountId}`, cookies });
  await app.close();
  return { response, worker };
}

describe("removing a connected account", () => {
  it("removes the credential and the history through the worker", async () => {
    const { response, worker } = await remove(`echo '{"ok":true,"removed":true,"broker":"binance"}'`, ME);
    expect(response.statusCode).toBe(200);
    expect(response.json()).toEqual({ removed: true, broker: "binance" });
    expect(worker.calls()).toEqual(["disconnect"]);
  });

  it("works for an additional account of mine, not only the first", async () => {
    expect((await remove(`echo '{"ok":true,"removed":true,"broker":"coinbase"}'`, `${ME}_coinbase`)).response.statusCode).toBe(200);
  });

  it("answers 404 when there is nothing to remove", async () => {
    expect((await remove(`echo '{"ok":true,"removed":false,"broker":"binance"}'`, ME)).response.statusCode).toBe(404);
    expect((await remove(`echo '{"ok":false,"error":"no connection for account x"}'; exit 1`, ME)).response.statusCode).toBe(404);
  });

  it("does not let one person remove another's account, and never reaches the worker", async () => {
    const { response, worker } = await remove(`echo '{"ok":true,"removed":true,"broker":"binance"}'`, SOMEONE_ELSE);
    expect(response.statusCode).toBe(403);
    expect(worker.calls()).toEqual([]);
  });

  it("requires signing in", async () => {
    const { response, worker } = await remove(`echo '{"ok":true,"removed":true}'`, ME, false);
    expect(response.statusCode).toBe(401);
    expect(worker.calls()).toEqual([]);
  });

  it("reports a worker failure without pretending the account is gone", async () => {
    const { response } = await remove(`echo '{"ok":false,"error":"database unreachable"}'; exit 1`, ME);
    expect(response.statusCode).toBe(502);
    expect(response.json().error).toBe("remove_failed");
  });
});
