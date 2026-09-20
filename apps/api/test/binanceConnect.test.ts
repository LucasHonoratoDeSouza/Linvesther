import { mkdtempSync, writeFileSync, chmodSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { invokeWorker, WorkerInvocationError } from "../src/binance-connect/worker.js";

// invokeWorker is tested against a small fake "worker binary" (a
// script implementing the same stdin-JSON-in, stdout-JSON-out
// contract) rather than the real Rust binary, so this suite runs fast
// and needs neither a Rust build nor a live Postgres — the real binary
// is exercised separately in tests/e2e's ignored real-connection test.

function fakeWorkerScript(behavior: "echo" | "list" | "fail" | "bad-json"): string {
  const dir = mkdtempSync(join(tmpdir(), "binance-worker-fake-"));
  const path = join(dir, "fake-worker.sh");
  let body: string;
  if (behavior === "echo") {
    body = `#!/bin/sh\ninput=$(cat)\necho "{\\"ok\\":true,\\"echoed\\":$input}"\n`;
  } else if (behavior === "list") {
    body = `#!/bin/sh\ninput=$(cat)\necho "{\\"ok\\":true,\\"accounts\\":[$input]}"\n`;
  } else if (behavior === "fail") {
    body = `#!/bin/sh\ncat > /dev/null\necho '{"ok":false,"error":"apiRestrictions grants a write/trade/transfer capability: enableWithdrawals"}'\n`;
  } else {
    body = `#!/bin/sh\ncat > /dev/null\necho 'not json'\n`;
  }
  writeFileSync(path, body);
  chmodSync(path, 0o755);
  return path;
}

describe("invokeWorker", () => {
  it("parses the worker's stdout JSON and resolves it", async () => {
    const script = fakeWorkerScript("echo");
    const result = await invokeWorker<{ ok: boolean; echoed: { accountId: string } }>({ binaryPath: script }, "connect", JSON.stringify({ accountId: "acc-1" }));
    expect(result.echoed.accountId).toBe("acc-1");
  });

  it("rejects with the worker's own error message when ok is false", async () => {
    const script = fakeWorkerScript("fail");
    await expect(invokeWorker({ binaryPath: script }, "connect", "{}")).rejects.toMatchObject({
      message: expect.stringContaining("enableWithdrawals"),
    });
  });

  it("rejects distinctly when the worker does not print valid JSON", async () => {
    const script = fakeWorkerScript("bad-json");
    await expect(invokeWorker({ binaryPath: script }, "connect", "{}")).rejects.toBeInstanceOf(WorkerInvocationError);
  });

  it("rejects when the binary does not exist at all", async () => {
    await expect(invokeWorker({ binaryPath: "/no/such/binary/here" }, "connect", "{}")).rejects.toBeInstanceOf(WorkerInvocationError);
  });
});

// A person can connect several exchange accounts under one identity:
// the account they signed in as, plus `<address>_<name>` for each
// further one. What must never happen is reaching anyone else's.
describe("owning several connected accounts", () => {
  const ME = "0x1111111111111111111111111111111111111111" as const;
  const SOMEONE_ELSE = "0x2222222222222222222222222222222222222222" as const;

  async function appWithSession(worker: "echo" | "list" = "echo") {
    const { buildApp } = await import("../src/app.js");
    const { MemorySessionStore } = await import("../src/auth/sessionStore.js");
    const sessionStore = new MemorySessionStore();
    const session = await sessionStore.create(ME);
    const app = buildApp({ domain: "localhost", sessionStore, binanceWorkerBinaryPath: fakeWorkerScript(worker) });
    await app.ready();
    return { app, cookies: { sid: session.id } };
  }

  const connect = (app: Awaited<ReturnType<typeof appWithSession>>["app"], cookies: { sid: string }, accountId: string) =>
    app.inject({
      method: "POST",
      url: `/accounts/${accountId}/binance-connection`,
      cookies,
      payload: { apiKey: "k", apiSecret: "s", label: "Swing" },
    });

  it("lets me connect my own address and any <address>_<name> of mine", async () => {
    const { app, cookies } = await appWithSession();
    expect((await connect(app, cookies, ME)).statusCode).toBe(201);
    const extra = await connect(app, cookies, `${ME}_swing-2`);
    expect(extra.statusCode).toBe(201);
    expect(extra.json().echoed).toMatchObject({ accountId: `${ME}_swing-2`, label: "Swing" });
  });

  it("refuses another person's account, and names that are not a plain short slug", async () => {
    const { app, cookies } = await appWithSession();
    expect((await connect(app, cookies, SOMEONE_ELSE)).statusCode).toBe(403);
    expect((await connect(app, cookies, `${SOMEONE_ELSE}_main`)).statusCode).toBe(403);
    expect((await connect(app, cookies, `${ME}extra`)).statusCode).toBe(403);
    expect((await connect(app, cookies, `${ME}_`)).statusCode).toBe(403);
    expect((await connect(app, cookies, `${ME}_${"a".repeat(33)}`)).statusCode).toBe(403);
    expect((await connect(app, cookies, `${ME}_bad%20name`)).statusCode).toBe(403);
  });

  it("lists accounts for the signed-in identity only, never a caller-chosen owner", async () => {
    const { app, cookies } = await appWithSession("list");
    const listed = await app.inject({ method: "GET", url: "/accounts", cookies });
    expect(listed.statusCode).toBe(200);
    expect(listed.json().accounts).toEqual([{ ownerAddress: ME }]);
    expect((await app.inject({ method: "GET", url: "/accounts" })).statusCode).toBe(401);
  });

  it("connects a Coinbase account for me only, sending the key to the worker for that exchange", async () => {
    const { app, cookies } = await appWithSession();
    const coinbase = (accountId: string, payload: object = { keyName: "organizations/o/apiKeys/k", privateKey: "PEM", label: "CB" }) =>
      app.inject({ method: "POST", url: `/accounts/${accountId}/coinbase-connection`, cookies, payload });
    const mine = await coinbase(`${ME}_coinbase`);
    expect(mine.statusCode).toBe(201);
    expect(mine.json().echoed).toMatchObject({ exchange: "coinbase", accountId: `${ME}_coinbase`, keyName: "organizations/o/apiKeys/k", label: "CB" });
    expect((await coinbase(`${SOMEONE_ELSE}_x`)).statusCode).toBe(403);
    expect((await coinbase(`${ME}_coinbase`, { keyName: "only-a-name" })).statusCode).toBe(400);
  });

  it("lets me rename my own accounts only", async () => {
    const { app, cookies } = await appWithSession();
    const rename = (accountId: string, label: string) => app.inject({ method: "PATCH", url: `/accounts/${accountId}/label`, cookies, payload: { label } });
    const mine = await rename(`${ME}_swing`, "  Long-term  ");
    expect(mine.statusCode).toBe(200);
    expect(mine.json().echoed).toMatchObject({ accountId: `${ME}_swing`, label: "Long-term" });
    expect((await rename(`${SOMEONE_ELSE}_x`, "Hi")).statusCode).toBe(403);
    expect((await rename(`${ME}_swing`, "   ")).statusCode).toBe(400);
  });

  it("connects an Interactive Brokers account for me only, sending the token to the worker for that exchange", async () => {
    const { app, cookies } = await appWithSession();
    const ibkr = (accountId: string, payload: object = { token: "tok-123", queryId: "998877", label: "IBKR" }) =>
      app.inject({ method: "POST", url: `/accounts/${accountId}/ibkr-connection`, cookies, payload });
    const mine = await ibkr(`${ME}_ibkr`);
    expect(mine.statusCode).toBe(201);
    expect(mine.json().echoed).toMatchObject({ exchange: "ibkr", accountId: `${ME}_ibkr`, token: "tok-123", queryId: "998877", label: "IBKR" });
    expect((await ibkr(`${SOMEONE_ELSE}_x`)).statusCode).toBe(403);
    expect((await ibkr(`${ME}_ibkr`, { token: "only-a-token" })).statusCode).toBe(400);
  });

  it("lets me refresh (sync) my own accounts only", async () => {
    const { app, cookies } = await appWithSession();
    const sync = (accountId: string, sid?: { sid: string }) => app.inject({ method: "POST", url: `/accounts/${accountId}/binance-sync`, cookies: sid });
    const mine = await sync(`${ME}_swing`, cookies);
    expect(mine.statusCode).toBe(200);
    expect(mine.json().echoed).toEqual({ accountId: `${ME}_swing` });
    expect((await sync(SOMEONE_ELSE, cookies)).statusCode).toBe(403);
    expect((await sync(ME)).statusCode).toBe(401);
  });

  it("serves the chart series for my own accounts, for known ranges only", async () => {
    const { app, cookies } = await appWithSession();
    const series = (accountId: string, range?: string, sid?: { sid: string }) =>
      app.inject({ method: "GET", url: `/accounts/${accountId}/binance-series${range ? `?range=${range}` : ""}`, cookies: sid });
    const mine = await series(ME, "24h", cookies);
    expect(mine.statusCode).toBe(200);
    expect(mine.json().echoed).toEqual({ accountId: ME, range: "24h" });
    expect((await series(ME, undefined, cookies)).json().echoed.range).toBe("max");
    expect((await series(ME, "2w", cookies)).statusCode).toBe(400);
    expect((await series(SOMEONE_ELSE, "24h", cookies)).statusCode).toBe(403);
    expect((await series(ME, "24h")).statusCode).toBe(401);
  });
});
