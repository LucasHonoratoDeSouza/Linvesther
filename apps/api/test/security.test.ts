import { chmodSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { buildApp } from "../src/app.js";
import { NONCE_CAPACITY, NONCE_TTL_MS, MemoryNonceStore } from "../src/auth/nonceStore.js";
import { RateLimiter } from "../src/auth/rateLimiter.js";
import { MemorySessionStore } from "../src/auth/sessionStore.js";
import { invokeWorker, WorkerBusyError } from "../src/binance-connect/worker.js";

const ME = "0x1111111111111111111111111111111111111111" as const;

function fakeWorker(script: string) {
  const dir = mkdtempSync(join(tmpdir(), "security-worker-"));
  const path = join(dir, "worker.sh");
  writeFileSync(path, `#!/bin/sh\ncat > /dev/null\n${script}\n`);
  chmodSync(path, 0o755);
  return path;
}

async function appWithSession(overrides: Partial<Parameters<typeof buildApp>[0]> = {}) {
  const sessionStore = new MemorySessionStore();
  const worker = fakeWorker(`echo '{"ok":true,"connectionId":"c","summary":{"tradesFetched":0,"flowsFetched":0,"catalogSymbolsFetched":0}}'`);
  const app = buildApp({ domain: "localhost", sessionStore, binanceWorkerBinaryPath: worker, ...overrides });
  await app.ready();
  const sid = async (address: `0x${string}` = ME) => ({ sid: (await sessionStore.create(address)).id });
  return { app, sid };
}

describe("sign-in challenges", () => {
  it("are long, random and never repeat", () => {
    const store = new MemoryNonceStore();
    const nonces = Array.from({ length: 200 }, () => store.issue());
    expect(new Set(nonces).size).toBe(200);
    for (const nonce of nonces) expect(nonce).toMatch(/^[A-Za-z0-9_-]{43}$/);
  });

  it("can be used once, and only if they were issued", () => {
    const store = new MemoryNonceStore();
    const nonce = store.issue();
    expect(store.consume("never-issued")).toBe(false);
    expect(store.consume(nonce)).toBe(true);
    expect(store.consume(nonce)).toBe(false);
  });

  it("stop working after a few minutes", () => {
    let now = 1_000;
    const store = new MemoryNonceStore(() => now);
    const nonce = store.issue();
    now += NONCE_TTL_MS + 1;
    expect(store.consume(nonce)).toBe(false);
  });

  it("cannot be requested without limit", () => {
    let now = 0;
    const store = new MemoryNonceStore(() => now);
    const first = store.issue();
    for (let i = 0; i < NONCE_CAPACITY + 50; i++) store.issue();
    // Past capacity the oldest was dropped instead of growing memory forever.
    expect(store.consume(first)).toBe(false);
    now += 1;
    expect(store.consume(store.issue())).toBe(true);
  });
});

describe("rate limiting", () => {
  it("forgets finished windows instead of remembering every client forever", () => {
    const limiter = new RateLimiter(1, 1_000);
    for (let i = 0; i < 10_050; i++) limiter.allow(`client-${i}`, 0);
    // A new window for an old client is granted again once the sweep has run.
    expect(limiter.allow("client-0", 5_000)).toBe(true);
    expect(limiter.allow("client-0", 5_001)).toBe(false);
  });

  it("limits sign-in attempts per client address, not for everyone", async () => {
    const { app } = await appWithSession();
    const attempt = (ip: string) => app.inject({ method: "POST", url: "/auth/vault/challenge", remoteAddress: ip });
    for (let i = 0; i < 30; i++) expect((await attempt("203.0.113.5")).statusCode).toBe(200);
    const blocked = await attempt("203.0.113.5");
    expect(blocked.statusCode).toBe(429);
    expect(blocked.json()).toEqual({ error: "rate_limited" });
    expect(blocked.headers["retry-after"]).toBeDefined();
    expect((await attempt("203.0.113.6")).statusCode).toBe(200);
    await app.close();
  });

  it("limits how often a client can make the server call an exchange", async () => {
    const { app, sid } = await appWithSession();
    const cookies = await sid();
    const connect = () =>
      app.inject({ method: "POST", url: `/accounts/${ME}/binance-connection`, cookies, payload: { apiKey: "k", apiSecret: "s" }, remoteAddress: "203.0.113.7" });
    for (let i = 0; i < 10; i++) expect((await connect()).statusCode).toBe(201);
    expect((await connect()).statusCode).toBe(429);
    await app.close();
  });

  it("limits how much unauthenticated reading one client can ask for", async () => {
    const { app } = await appWithSession();
    const read = () => app.inject({ method: "GET", url: "/public/collector", remoteAddress: "203.0.113.8" });
    for (let i = 0; i < 120; i++) expect((await read()).statusCode).toBe(200);
    expect((await read()).statusCode).toBe(429);
    await app.close();
  });

  it("can be scaled for a test environment that signs in faster than a person", async () => {
    const { app } = await appWithSession({ limitMultiplier: 2 });
    // The sign-in limit is 30 a minute; doubled it is 60.
    for (let i = 0; i < 60; i++) {
      expect((await app.inject({ method: "POST", url: "/auth/vault/challenge", remoteAddress: "203.0.113.50" })).statusCode).toBe(200);
    }
    expect((await app.inject({ method: "POST", url: "/auth/vault/challenge", remoteAddress: "203.0.113.50" })).statusCode).toBe(429);
    await app.close();
  });

  it("cannot be dodged by forging a forwarding header when behind one proxy", async () => {
    const { app } = await appWithSession({ trustProxyHops: 1 });
    const attempt = (forged: string) =>
      app.inject({ method: "POST", url: "/auth/vault/challenge", remoteAddress: "127.0.0.1", headers: { "x-forwarded-for": `${forged}, 198.51.100.9` } });
    for (let i = 0; i < 30; i++) expect((await attempt(`10.0.0.${i}`)).statusCode).toBe(200);
    // Thirty different forged addresses, one real one: still the same client.
    expect((await attempt("10.0.1.1")).statusCode).toBe(429);
    await app.close();
  });

  it("gives a proof quota to a person, however many sessions they open", async () => {
    const { app, sid } = await appWithSession({ proofRateLimiter: new RateLimiter(1, 60_000) });
    const [first, second] = [await sid(), await sid()];
    const proof = (cookies: { sid: string }) => app.inject({ method: "GET", url: `/accounts/${ME}/binance-performance-proof`, cookies });
    expect((await proof(first)).statusCode).not.toBe(429);
    expect((await proof(second)).statusCode).toBe(429);
    await app.close();
  });
});

describe("the operator's gas budget", () => {
  it("is shared by every client, so many of them together cannot drain it", async () => {
    const { app, sid } = await appWithSession({ relayBudgetPerHour: 3 });
    const cookies = await sid();
    const relay = (ip: string) => app.inject({ method: "POST", url: "/identities", cookies, remoteAddress: ip });
    // Different clients, each far below its own limit.
    for (const ip of ["203.0.113.21", "203.0.113.22", "203.0.113.23"]) expect((await relay(ip)).statusCode).not.toBe(503);
    const refused = await relay("203.0.113.24");
    expect(refused.statusCode).toBe(503);
    expect(refused.json()).toEqual({ error: "relay_budget_exhausted" });
    await app.close();
  });

  it("is not spent by reading", async () => {
    const { app } = await appWithSession({ relayBudgetPerHour: 1 });
    for (let i = 0; i < 5; i++) expect((await app.inject({ method: "GET", url: "/public/collector", remoteAddress: "203.0.113.30" })).statusCode).toBe(200);
    await app.close();
  });
});

describe("what a client is told", () => {
  it("never sees the text of an internal failure", async () => {
    const app = buildApp({ domain: "localhost" });
    app.get("/boom", async () => {
      throw new Error("connection string postgres://user:hunter2@db/prod");
    });
    await app.ready();
    const response = await app.inject({ method: "GET", url: "/boom" });
    expect(response.statusCode).toBe(500);
    expect(response.json()).toEqual({ error: "internal_error" });
    expect(response.body).not.toContain("hunter2");
    await app.close();
  });

  it("keeps a client mistake as a client error", async () => {
    const { app } = await appWithSession();
    const response = await app.inject({ method: "POST", url: "/auth/vault/verify", headers: { "content-type": "application/json" }, payload: "{not json" });
    expect(response.statusCode).toBe(400);
    await app.close();
  });

  it("is never cached or sniffed", async () => {
    const { app } = await appWithSession();
    const response = await app.inject({ method: "GET", url: "/public/collector" });
    expect(response.headers["cache-control"]).toBe("no-store");
    expect(response.headers["x-content-type-options"]).toBe("nosniff");
    expect(response.headers["referrer-policy"]).toBe("no-referrer");
    await app.close();
  });
});

describe("credentials sent to connect an account", () => {
  const connect = async (path: string, payload: unknown) => {
    const { app, sid } = await appWithSession();
    const response = await app.inject({ method: "POST", url: `/accounts/${ME}/${path}`, cookies: await sid(), payload: payload as never });
    await app.close();
    return response;
  };

  it("must be plain, bounded strings", async () => {
    for (const bad of [{}, { apiKey: "k" }, { apiKey: { $ne: 1 }, apiSecret: "s" }, { apiKey: "k", apiSecret: 5 }, { apiKey: "k".repeat(300), apiSecret: "s" }, { apiKey: "k", apiSecret: "s", label: 7 }, { apiKey: "k", apiSecret: "s", symbols: "BTCUSDT" }]) {
      expect((await connect("binance-connection", bad)).statusCode).toBe(400);
    }
    expect((await connect("coinbase-connection", { keyName: "n", privateKey: "x".repeat(5000) })).statusCode).toBe(400);
    expect((await connect("ibkr-connection", { token: "t", queryId: ["1"] })).statusCode).toBe(400);
  });

  it("are refused, not crashed on, when there is no body at all", async () => {
    const { app, sid } = await appWithSession();
    const response = await app.inject({ method: "POST", url: `/accounts/${ME}/binance-connection`, cookies: await sid() });
    expect(response.statusCode).toBe(400);
    await app.close();
  });

  it("are accepted when well formed", async () => {
    expect((await connect("binance-connection", { apiKey: "k", apiSecret: "s", symbols: ["BTCUSDT"], label: "Main" })).statusCode).toBe(201);
  });
});

describe("the worker process", () => {
  it("is not started without limit: past the cap a call is turned away", async () => {
    const path = fakeWorker(`sleep 0.4\necho '{"ok":true}'`);
    const calls = Array.from({ length: 10 }, () => invokeWorker({ binaryPath: path }, "list", "{}").then(() => "ok", (error) => error));
    const results = await Promise.all(calls);
    expect(results.filter((r) => r === "ok")).toHaveLength(8);
    const rejected = results.filter((r) => r !== "ok");
    expect(rejected).toHaveLength(2);
    for (const error of rejected) expect(error).toBeInstanceOf(WorkerBusyError);
    // Finished calls give their place back.
    await expect(invokeWorker({ binaryPath: path }, "list", "{}")).resolves.toBeDefined();
  });
});
