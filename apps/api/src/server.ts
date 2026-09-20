// Standalone HTTP entry point, used by the e2e web suite to run a real
// apps/api instance for apps/web to call over the network — buildApp()
// itself is transport-agnostic (tests elsewhere drive it via
// `.inject()`), this file is the only place that actually binds a port.

import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { createPublicClient, http } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { Pool } from "pg";
import {
  Indexer,
  MemoryRelayStore,
  PostgresProjectionStore,
  Relayer,
  ViemBroadcastClient,
  ViemChainClient,
  runMigrations,
} from "@linvestherzk/chain-worker";
import { buildApp, type AppOptions } from "./app.js";
import { createPool } from "./storage/pool.js";
import { credentialStore, useCredentialStore } from "./auth/identitySignature.js";
import { PostgresCredentialStore, PostgresWebAuthnCredentialStore } from "./auth/credentialStore.js";
import { PostgresNonceStore } from "./auth/nonceStore.js";
import { PostgresRateLimiter, type RateLimit } from "./auth/rateLimiter.js";
import { useVaultNonceStore } from "./auth/vault.js";
import { MemorySessionStore, PostgresSessionStore, type SessionStore } from "./auth/sessionStore.js";
import { useWebAuthnCredentialStore, useWebAuthnNonceStores } from "./auth/webauthn.js";
import { MemoryDisclosureStore, PostgresDisclosureStore, type DisclosureStore } from "./claims/store.js";
import { MemoryPublicTrackStore } from "./public/store.js";
import { MemorySettingsStore, PostgresSettingsStore, type SettingsStore } from "./profile/store.js";

const port = Number(process.env.PORT ?? 4301);
const host = process.env.HOST ?? "127.0.0.1";
const domain = process.env.SIWE_DOMAIN ?? "localhost";
const corsOrigins = (process.env.CORS_ORIGINS ?? "").split(",").filter(Boolean);

// app.ts's own default ("services/target/debug/binance-worker") is
// relative to whatever the process's cwd happens to be at spawn time —
// fine for tests, which always pass their own correct relative path,
// but wrong the moment this file is actually run as a long-lived
// server from a directory other than the repo root (e.g. `cd apps/api
// && ...`). Resolved from this file's own location instead, so it's
// correct regardless of cwd.
const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const binanceWorkerBinaryPath = process.env.BINANCE_WORKER_BINARY_PATH ?? resolve(repoRoot, "services/target/debug/binance-worker");

/** Constructs the real on-chain identity lifecycle deps from environment variables —
 * `undefined` unless every required one is set, so a deployment/dev run
 * that hasn't configured a chain yet still starts with `/identities*`
 * simply not registered (see app.ts), not a crash. */
async function loadChainLifecycle(): Promise<AppOptions["chainLifecycle"]> {
  const rpcUrl = process.env.CHAIN_RPC_URL;
  const relayerPrivateKey = process.env.CHAIN_RELAYER_PRIVATE_KEY as `0x${string}` | undefined;
  const identityRegistry = process.env.IDENTITY_REGISTRY_ADDRESS as `0x${string}` | undefined;
  const accountRegistry = process.env.ACCOUNT_REGISTRY_ADDRESS as `0x${string}` | undefined;
  const accountFactory = process.env.ACCOUNT_FACTORY_ADDRESS as `0x${string}` | undefined;
  const databaseUrl = process.env.DATABASE_URL;
  if (!rpcUrl || !relayerPrivateKey || !identityRegistry || !accountRegistry || !accountFactory || !databaseUrl) {
    return undefined;
  }
  const deployBlock = BigInt(process.env.CHAIN_DEPLOY_BLOCK ?? "0");

  const pool = createPool(databaseUrl);
  await runMigrations(pool);

  const relayerAccount = privateKeyToAccount(relayerPrivateKey);
  const relayer = new Relayer(new ViemBroadcastClient(rpcUrl, relayerPrivateKey), new MemoryRelayStore(), undefined, BigInt(process.env.RELAYER_MIN_BALANCE_WEI ?? "1000000000000"));
  const publicClient = createPublicClient({ transport: http(rpcUrl) });
  const projectionStore = new PostgresProjectionStore(pool, publicClient, [identityRegistry, accountRegistry]);

  // Resume from wherever a previous process already indexed to
  // (Postgres persists across restarts, the in-memory Indexer doesn't)
  // — only a genuinely first-ever run falls back to deployBlock.
  const resumeFrom = (await projectionStore.latestIndexedBlock()) ?? deployBlock;

  return {
    relayFlowDeps: { relayer, relayerAccount, publicClient, identityRegistry, accountRegistry, accountFactory },
    projectionStore,
    indexer: new Indexer(projectionStore),
    chainClient: new ViemChainClient(rpcUrl),
    credentialStore,
    profileRegistry: process.env.PROFILE_REGISTRY_ADDRESS as `0x${string}` | undefined,
    deployBlock: resumeFrom,
  };
}

const publicTrackStore = new MemoryPublicTrackStore();
if (process.env.DEMO_SEED === "1") {
  // Test-only fixture so the e2e web suite has something real to fetch
  // from GET /public/tracks/:trackId — never enabled outside a local
  // e2e run (no production deployment sets DEMO_SEED).
  publicTrackStore.save({
    identityId: "id-1",
    trackId: "track-1",
    createdAt: "2026-01-01T00:00:00Z",
    verifiedSince: "2026-01-05T00:00:00Z",
    currency: "USDT",
    originMechanism: "A0",
    coverageStatus: "POLICY_COMPLETE",
    calculationStatus: "ZK_VERIFIED",
    registryStatus: "FINALIZED",
    availabilityStatus: "AVAILABLE",
    gaps: [],
    corrections: [],
    exactBalanceUsd: "0",
    operations: [],
    sourceApiKeyDigest: "",
    institutionWalletId: "",
    witnessBlob: "",
    sourceNamespaceUid: "",
  });
}

// Who is signed in, who registered, what people published and what they chose
// to hide must all outlive a restart, so with a database they live there.
// Without one (a quick local run) they are kept in memory and lost on restart.
let sessionStore: SessionStore = new MemorySessionStore();
let disclosureStore: DisclosureStore = new MemoryDisclosureStore();
let profileSettingsStore: SettingsStore = new MemorySettingsStore();
let limiterFactory: ((name: string, max: number, windowMs: number) => RateLimit) | undefined;
let proofRateLimiter: RateLimit | undefined;
if (process.env.DATABASE_URL) {
  const pool = createPool(process.env.DATABASE_URL);
  const sessions = new PostgresSessionStore(pool);
  const credentials = new PostgresCredentialStore(pool);
  const passkeys = new PostgresWebAuthnCredentialStore(pool);
  const disclosures = new PostgresDisclosureStore(pool);
  const settings = new PostgresSettingsStore(pool);
  await Promise.all([sessions.ensureSchema(), credentials.ensureSchema(), passkeys.ensureSchema(), disclosures.ensureSchema(), settings.ensureSchema()]);
  // Challenges and traffic limits are shared too, so several instances behave
  // as one and a restart forgets nothing.
  const nonces = { vault: new PostgresNonceStore(pool, "vault"), registration: new PostgresNonceStore(pool, "webauthn-registration"), authentication: new PostgresNonceStore(pool, "webauthn-authentication") };
  await Promise.all([nonces.vault.ensureSchema(), nonces.registration.ensureSchema(), nonces.authentication.ensureSchema()]);
  useVaultNonceStore(nonces.vault);
  useWebAuthnNonceStores({ registration: nonces.registration, authentication: nonces.authentication });
  await new PostgresRateLimiter(pool, "schema", 1, 1).ensureSchema();
  limiterFactory = (name, max, windowMs) => new PostgresRateLimiter(pool, name, max, windowMs);
  proofRateLimiter = limiterFactory("proof", 3, 10 * 60_000);
  useCredentialStore(credentials);
  useWebAuthnCredentialStore(passkeys);
  sessionStore = sessions;
  disclosureStore = disclosures;
  profileSettingsStore = settings;
} else {
  console.warn("DATABASE_URL is not set: sign-ins, identities and claims are kept in memory and are lost when the API restarts");
}

// Loaded after the credential store is chosen, since it captures that store.
const chainLifecycle = await loadChainLifecycle();

const app = buildApp({ domain, trustProxyHops: Number(process.env.TRUST_PROXY_HOPS ?? 0), limiterFactory, proofRateLimiter, limitMultiplier: process.env.TRAFFIC_LIMIT_MULTIPLIER ? Number(process.env.TRAFFIC_LIMIT_MULTIPLIER) : undefined, relayBudgetPerHour: process.env.RELAY_MAX_PER_HOUR ? Number(process.env.RELAY_MAX_PER_HOUR) : undefined, publicTrackStore, sessionStore, disclosureStore, secureCookies: (process.env.WEBAUTHN_ORIGIN ?? "").startsWith("https://"), corsOrigins, chainLifecycle, binanceWorkerBinaryPath, profileSettingsStore });

if (chainLifecycle) {
  // Keeps the indexer's local view continuously close to the chain tip,
  // independent of lifecycle requests. Without this, the indexer only
  // advances in response to a write (see chainLifecycle.ts's resync
  // calls) — after any idle stretch, the very next request would have
  // to walk the whole accumulated gap itself before it could read back
  // its own change, a multi-minute wait against a real, block-by-block
  // indexed chain. Errors are logged, never fatal — the next tick
  // retries, and lifecycle calls' own resyncUntil still covers the gap
  // this misses.
  //
  // Scheduled as "wait, then run, then reschedule" rather than a plain
  // setInterval: a real chain's catch-up sync can itself take longer
  // than the tick interval, and setInterval fires on a fixed clock
  // regardless — overlapping sync() calls would race the same
  // in-memory Indexer.chain and the same Postgres rows concurrently,
  // which slows catch-up down rather than speeding it up.
  const backgroundSync = () => {
    chainLifecycle.indexer
      .sync(chainLifecycle.chainClient, chainLifecycle.deployBlock)
      .catch((error: unknown) => {
        console.error("background indexer sync failed", error);
      })
      .finally(() => {
        setTimeout(backgroundSync, 3_000);
      });
  };
  setTimeout(backgroundSync, 3_000);
}

app
  .listen({ port, host })
  .then(() => {
    console.log(`apps/api listening on http://${host}:${port}${chainLifecycle ? " (on-chain identity enabled)" : ""}`);
  })
  .catch((error: unknown) => {
    console.error(error);
    process.exit(1);
  });
