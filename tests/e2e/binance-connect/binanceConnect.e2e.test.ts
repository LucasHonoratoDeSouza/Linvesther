import { buildApp } from "@linvestherzk/api";
import { concat, getAddress, keccak256, slice } from "viem";
import { afterEach, describe, expect, it } from "vitest";

// End-to-end: does connecting a Binance account actually work through
// the real product surface (HTTP route -> real Rust worker binary ->
// real Postgres -> real Binance), not just as a manually-run script?
//
// This is a real, credentialed test against a live Binance account and
// a running local Postgres, so — same as every other real-credential
// test in this repository — it is SKIPPED unless the required
// environment is present, rather than failing the default gate. Run it
// for real with:
//
//   docker compose -f infra/docker-compose.yml up -d postgres
//   cd services && cargo build -p binance-worker
//   set -a && source ../.env && set +a
//   export DATABASE_URL="postgresql://linvestherzk:linvestherzk-local-dev-only@localhost:5433/linvestherzk"
//   export BINANCE_WORKER_ENCRYPTION_KEY=$(openssl rand -hex 32)
//   pnpm --filter e2e-tests test -- binance-connect

const DOMAIN = "app.linvestherzk.example";
const ACCOUNT_ID = "e2e-real-binance-account";
const NOW = new Date("2026-01-01T00:05:00Z");

const hasRealCredentials = Boolean(process.env.BINANCE_API_KEY && process.env.BINANCE_API_SECRET && process.env.DATABASE_URL && process.env.BINANCE_WORKER_ENCRYPTION_KEY);

interface VaultKeyPair {
  privateKey: CryptoKey;
  qx: `0x${string}`;
  qy: `0x${string}`;
  address: `0x${string}`;
}

function bytesToHex(bytes: Uint8Array): `0x${string}` {
  return `0x${Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("")}`;
}

function expectedSubjectKey(qx: `0x${string}`, qy: `0x${string}`): `0x${string}` {
  return getAddress(slice(keccak256(concat([qx, qy])), 12));
}

async function generateVaultKeyPair(): Promise<VaultKeyPair> {
  const { publicKey, privateKey } = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, [
    "sign",
    "verify",
  ]);
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", publicKey));
  const qx = bytesToHex(raw.slice(1, 33));
  const qy = bytesToHex(raw.slice(33, 65));
  return { privateKey, qx, qy, address: expectedSubjectKey(qx, qy) };
}

async function signIn(app: Awaited<ReturnType<typeof buildApp>>, keyPair: VaultKeyPair) {
  const challengeResponse = await app.inject({ method: "POST", url: "/auth/vault/challenge" });
  const { challenge } = challengeResponse.json() as { challenge: string };
  const signature = bytesToHex(
    new Uint8Array(
      await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, keyPair.privateKey, new TextEncoder().encode(challenge)),
    ),
  );
  const verifyResponse = await app.inject({
    method: "POST",
    url: "/auth/vault/verify",
    payload: { qx: keyPair.qx, qy: keyPair.qy, challenge, signature },
  });
  const setCookie = verifyResponse.headers["set-cookie"];
  const cookie = Array.isArray(setCookie) ? setCookie[0] : setCookie;
  return cookie!.split(";")[0]!.split("=")[1]!;
}

describe.skipIf(!hasRealCredentials)("Real Binance connection, end to end through the product", () => {
  let app: Awaited<ReturnType<typeof buildApp>>;

  afterEach(async () => {
    await app?.close();
  });

  it(
    "a person connects their real read-only Binance key through the actual HTTP route and it works",
    async () => {
      const owner = await generateVaultKeyPair();
      const accountOwners = new Map<string, `0x${string}`>([[ACCOUNT_ID, owner.address]]);
      app = buildApp({ domain: DOMAIN, now: () => NOW, accountOwners, binanceWorkerBinaryPath: "../../services/target/debug/binance-worker" });
      await app.ready();
      const sid = await signIn(app, owner);

      const connectResponse = await app.inject({
        method: "POST",
        url: `/accounts/${ACCOUNT_ID}/binance-connection`,
        cookies: { sid },
        payload: { apiKey: process.env.BINANCE_API_KEY, apiSecret: process.env.BINANCE_API_SECRET, symbols: ["BTCUSDT"] },
      });

      expect(connectResponse.statusCode).toBe(201);
      const body = connectResponse.json() as { connectionId: string; summary: { tradesFetched: number } };
      expect(body.connectionId).toBeTruthy();

      const statusResponse = await app.inject({ method: "GET", url: `/accounts/${ACCOUNT_ID}/binance-connection`, cookies: { sid } });
      expect(statusResponse.statusCode).toBe(200);
      const status = statusResponse.json() as { connected: boolean };
      expect(status.connected).toBe(true);

      // Real NAV, from the same connection, computed from the real
      // balance and real prices — not a historical series (see
      // services/collector/binance/worker/src/publish.rs).
      const navResponse = await app.inject({ method: "GET", url: `/accounts/${ACCOUNT_ID}/binance-nav`, cookies: { sid } });
      expect(navResponse.statusCode).toBe(200);
      const nav = navResponse.json() as { nav: string; currency: string; assets: unknown[] };
      expect(nav.currency).toBe("USDT");
      expect(Number(nav.nav)).toBeGreaterThan(0);
    },
    30_000,
  );

  it(
    "real historical performance (Sharpe/Sortino/MDD/CAGR/win-rate) computes through the actual HTTP route",
    async () => {
      const owner = await generateVaultKeyPair();
      const accountOwners = new Map<string, `0x${string}`>([[ACCOUNT_ID, owner.address]]);
      app = buildApp({ domain: DOMAIN, now: () => NOW, accountOwners, binanceWorkerBinaryPath: "../../services/target/debug/binance-worker" });
      await app.ready();
      const sid = await signIn(app, owner);

      await app.inject({
        method: "POST",
        url: `/accounts/${ACCOUNT_ID}/binance-connection`,
        cookies: { sid },
        payload: { apiKey: process.env.BINANCE_API_KEY, apiSecret: process.env.BINANCE_API_SECRET, symbols: ["BTCUSDT", "SOLUSDT"] },
      });

      const performanceResponse = await app.inject({ method: "GET", url: `/accounts/${ACCOUNT_ID}/binance-performance`, cookies: { sid } });
      expect(performanceResponse.statusCode).toBe(200);
      const performance = performanceResponse.json() as {
        dailyNav: { dayStartMs: number; nav: string }[];
        dailyReturns: string[];
        cagr: string | null;
        cagrUnavailable: string | null;
      };
      expect(performance.dailyNav.length).toBeGreaterThan(0);
      expect(performance.dailyReturns.length).toBeGreaterThan(0);
      // This real account's real history is under 365 days — CAGR must
      // refuse honestly, with a reason, never a fabricated number.
      expect(performance.cagr).toBeNull();
      expect(performance.cagrUnavailable).toBeTruthy();
    },
    120_000,
  );

  it("a third party cannot connect an account that is not theirs", async () => {
    const owner = await generateVaultKeyPair();
      const accountOwners = new Map<string, `0x${string}`>([[ACCOUNT_ID, owner.address]]);
    app = buildApp({ domain: DOMAIN, now: () => NOW, accountOwners, binanceWorkerBinaryPath: "../../services/target/debug/binance-worker" });
    await app.ready();
    const thirdParty = await generateVaultKeyPair();
    const sid = await signIn(app, thirdParty);

    const response = await app.inject({
      method: "POST",
      url: `/accounts/${ACCOUNT_ID}/binance-connection`,
      cookies: { sid },
      payload: { apiKey: process.env.BINANCE_API_KEY, apiSecret: process.env.BINANCE_API_SECRET, symbols: ["BTCUSDT"] },
    });
    expect(response.statusCode).toBe(403);
  });

  it("a third party cannot fetch a real ZK performance proof for an account that is not theirs", async () => {
    // No real proving is ever attempted here — the ownership check
    // rejects the request before invokeWorker runs, so this stays fast
    // even though the real path (see binance-worker/README.md's "Real
    // ZK performance proof") can take a very long time.
    const owner = await generateVaultKeyPair();
      const accountOwners = new Map<string, `0x${string}`>([[ACCOUNT_ID, owner.address]]);
    app = buildApp({ domain: DOMAIN, now: () => NOW, accountOwners, binanceWorkerBinaryPath: "../../services/target/debug/binance-worker" });
    await app.ready();
    const thirdParty = await generateVaultKeyPair();
    const sid = await signIn(app, thirdParty);

    const response = await app.inject({ method: "GET", url: `/accounts/${ACCOUNT_ID}/binance-performance-proof`, cookies: { sid } });
    expect(response.statusCode).toBe(403);
  });
});
