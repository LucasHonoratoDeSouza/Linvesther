import { readFileSync } from "node:fs";
import path from "node:path";
import { type ChildProcess, spawn } from "node:child_process";
import {
  buildApp,
  credentialStore,
  type ChainLifecycleDeps,
} from "@linvestherzk/api";
import {
  Indexer,
  MemoryRelayStore,
  Relayer,
  ViemBroadcastClient,
  ViemChainClient,
  PostgresProjectionStore,
  runMigrations,
  accountFactoryAbi,
  accountRegistryAbi,
  identityRegistryAbi,
} from "@linvestherzk/chain-worker";
import { createPublicClient, createWalletClient, hashTypedData, http, keccak256, toHex, type Hex } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { Pool } from "pg";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";

// End-to-end against the real relayed identity flow a real local Anvil, a real
// local Postgres, and the real deployed contracts — driven through the
// real Fastify app via `.inject()`, not a simulation.

const DOMAIN = "app.linvestherzk.example";
const PORT = 8850;
const RPC_URL = `http://127.0.0.1:${PORT}`;
const DATABASE_URL = process.env.DATABASE_URL ?? "postgresql://linvestherzk:linvestherzk-local-dev-only@localhost:5433/linvestherzk";
const DEPLOYER_KEY = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80" as const;
const CONTRACTS_OUT = path.resolve(import.meta.dirname, "../../../contracts/out");

interface VaultKeyPair {
  privateKey: CryptoKey;
  qx: `0x${string}`;
  qy: `0x${string}`;
}

function bytesToHex(bytes: Uint8Array): `0x${string}` {
  return `0x${Array.from(bytes).map((b) => b.toString(16).padStart(2, "0")).join("")}`;
}

const P256_N = 0xffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551n;

async function generateVaultKeyPair(): Promise<VaultKeyPair> {
  const { publicKey, privateKey } = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, ["sign", "verify"]);
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", publicKey));
  return { privateKey, qx: bytesToHex(raw.slice(1, 33)), qy: bytesToHex(raw.slice(33, 65)) };
}

/** Signs `message` (used for the login challenge, a plain string) or a
 * raw 32-byte digest (used for the confirm-rotation challenge) the way
 * the real vault does — see chain-worker's testChain.ts for why `s`
 * needs low-form normalization and why the raw digest bytes (not their
 * UTF-8 text) must be signed for an on-chain digest. */
async function signChallengeString(privateKey: CryptoKey, message: string): Promise<`0x${string}`> {
  const signature = await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, privateKey, new TextEncoder().encode(message));
  return bytesToHex(new Uint8Array(signature));
}

async function signDigest(privateKey: CryptoKey, digest: `0x${string}`): Promise<`0x${string}`> {
  const bytes = new Uint8Array(digest.slice(2).match(/.{2}/g)!.map((b) => Number.parseInt(b, 16)));
  const raw = new Uint8Array(await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, privateKey, bytes));
  const r = raw.slice(0, 32);
  let s = BigInt(bytesToHex(raw.slice(32, 64)));
  if (s > P256_N / 2n) s = P256_N - s;
  return `0x${bytesToHex(r).slice(2)}${s.toString(16).padStart(64, "0")}`;
}

function bytecode(contractFile: string, contractName: string): Hex {
  const artifact = JSON.parse(readFileSync(path.join(CONTRACTS_OUT, `${contractFile}.sol`, `${contractName}.json`), "utf8")) as {
    bytecode: { object: Hex };
  };
  return artifact.bytecode.object;
}

let anvil: ChildProcess;
let pool: Pool;
let chainLifecycle: ChainLifecycleDeps;

beforeAll(async () => {
  anvil = spawn("anvil", ["--port", String(PORT), "--silent"], { stdio: "ignore" });
  for (let attempt = 0; attempt < 50; attempt++) {
    try {
      await fetch(RPC_URL, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "eth_blockNumber", params: [] }) });
      break;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }

  const deployerAccount = privateKeyToAccount(DEPLOYER_KEY);
  const wallet = createWalletClient({ account: deployerAccount, transport: http(RPC_URL) });
  const publicClient = createPublicClient({ transport: http(RPC_URL) });

  async function deploy(contractFile: string, contractName: string, abi: readonly unknown[], args: unknown[] = []) {
    const hash = await wallet.deployContract({ chain: undefined, abi: abi as never, bytecode: bytecode(contractFile, contractName), args: args as never });
    const receipt = await publicClient.waitForTransactionReceipt({ hash });
    return receipt.contractAddress!;
  }

  const identityRegistry = await deploy("IdentityRegistry", "IdentityRegistry", identityRegistryAbi);
  const accountRegistry = await deploy("AccountRegistry", "AccountRegistry", accountRegistryAbi, [identityRegistry]);
  const webAuthnImpl = await deploy("WebAuthnAccount", "WebAuthnAccount", []);
  const p256VaultImpl = await deploy("P256VaultAccount", "P256VaultAccount", []);
  const accountFactory = await deploy("AccountFactory", "AccountFactory", accountFactoryAbi, [webAuthnImpl, p256VaultImpl]);

  pool = new Pool({ connectionString: DATABASE_URL });
  await pool.query("DROP TABLE IF EXISTS accounts, tracks, identities, indexed_blocks, schema_migrations CASCADE");
  await runMigrations(pool);

  const relayerAccount = privateKeyToAccount(DEPLOYER_KEY);
  const relayer = new Relayer(new ViemBroadcastClient(RPC_URL, DEPLOYER_KEY), new MemoryRelayStore());
  const projectionStore = new PostgresProjectionStore(pool, publicClient, [identityRegistry, accountRegistry]);
  const indexer = new Indexer(projectionStore);
  const chainClient = new ViemChainClient(RPC_URL);

  chainLifecycle = {
    relayFlowDeps: { relayer, relayerAccount, publicClient, identityRegistry, accountFactory, accountRegistry },
    projectionStore,
    indexer,
    chainClient,
    credentialStore,
    deployBlock: 0n,
  };
}, 60_000);

afterAll(async () => {
  anvil.kill();
  await pool.end();
});

async function buildTestApp() {
  const app = buildApp({ domain: DOMAIN, chainLifecycle });
  await app.ready();
  return app;
}

async function signIn(app: Awaited<ReturnType<typeof buildTestApp>>, keyPair: VaultKeyPair) {
  const challengeResponse = await app.inject({ method: "POST", url: "/auth/vault/challenge" });
  const { challenge } = challengeResponse.json() as { challenge: string };
  const signature = await signChallengeString(keyPair.privateKey, challenge);
  const verifyResponse = await app.inject({ method: "POST", url: "/auth/vault/verify", payload: { qx: keyPair.qx, qy: keyPair.qy, challenge, signature } });
  const setCookie = verifyResponse.headers["set-cookie"];
  const cookie = Array.isArray(setCookie) ? setCookie[0] : setCookie;
  return cookie!.split(";")[0]!.split("=")[1]!;
}

interface WireChallenge {
  domain: unknown;
  types: unknown;
  primaryType: string;
  message: Record<string, unknown> & { ownerEpoch: string; nonce: string; deadline: string };
}

/** Reverses the bigint-to-string wire encoding every challenge endpoint
 * uses (JSON has no bigint type), signs the resulting EIP-712 digest,
 * and POSTs the action — the same round trip a real client does. */
async function signAndSend(app: Awaited<ReturnType<typeof buildTestApp>>, sid: string, actionUrl: string, wireChallenge: WireChallenge, privateKey: CryptoKey) {
  // Every uint64/uint256 field (ownerEpoch/nonce/deadline always, plus
  // e.g. ActivateAccount's eligibleFrom) crosses the wire as a decimal
  // string — convert every purely-numeric string back to bigint rather
  // than hardcoding which challenge type has which extra field.
  const message = Object.fromEntries(
    Object.entries(wireChallenge.message).map(([key, value]) => [key, typeof value === "string" && /^\d+$/.test(value) ? BigInt(value) : value]),
  );
  const challenge = { ...wireChallenge, message };
  const digest = hashTypedData(challenge as Parameters<typeof hashTypedData>[0]);
  const signature = await signDigest(privateKey, digest);
  return app.inject({ method: "POST", url: actionUrl, cookies: { sid }, payload: { challenge: wireChallenge, signature } });
}

describe("Real on-chain identity creation and confirmation", () => {
  let app: Awaited<ReturnType<typeof buildTestApp>>;

  afterEach(async () => {
    await app?.close();
  });

  it("creating an identity deploys the real account and proposes it as owner, starting pending_confirmation", async () => {
    app = await buildTestApp();
    const owner = await generateVaultKeyPair();
    const sid = await signIn(app, owner);

    const response = await app.inject({ method: "POST", url: "/identities", cookies: { sid } });
    expect(response.statusCode).toBe(202);
    const identity = response.json() as { identityId: `0x${string}`; state: string; pendingOwner: `0x${string}` | null };
    expect(identity.state).toBe("pending_confirmation");
    expect(identity.pendingOwner).not.toBeNull();

    const read = await app.inject({ method: "GET", url: `/identities/${identity.identityId}` });
    expect((read.json() as { state: string }).state).toBe("pending_confirmation");
  }, 30_000);

  it("confirming with the real vault signature transfers ownership to the account, ending active", async () => {
    app = await buildTestApp();
    const owner = await generateVaultKeyPair();
    const sid = await signIn(app, owner);

    const created = (await app.inject({ method: "POST", url: "/identities", cookies: { sid } })).json() as {
      identityId: `0x${string}`;
    };

    const challengeResponse = await app.inject({ method: "POST", url: `/identities/${created.identityId}/confirm-challenge`, cookies: { sid } });
    expect(challengeResponse.statusCode).toBe(200);
    const wireChallenge = challengeResponse.json() as {
      domain: { name: string; version: string; chainId: number; verifyingContract: `0x${string}` };
      types: { ConfirmOwnerRotation: { name: string; type: string }[] };
      primaryType: "ConfirmOwnerRotation";
      message: { identityId: `0x${string}`; newOwner: `0x${string}`; ownerEpoch: string; nonce: string; deadline: string };
    };
    // ownerEpoch/nonce/deadline cross the wire as decimal strings (JSON
    // has no bigint) — hashTypedData needs the real bigint values back.
    const challenge = {
      ...wireChallenge,
      message: {
        ...wireChallenge.message,
        ownerEpoch: BigInt(wireChallenge.message.ownerEpoch),
        nonce: BigInt(wireChallenge.message.nonce),
        deadline: BigInt(wireChallenge.message.deadline),
      },
    };

    const eip712Digest = hashTypedData(challenge);
    const signature = await signDigest(owner.privateKey, eip712Digest);

    const confirmResponse = await app.inject({
      method: "POST",
      url: `/identities/${created.identityId}/confirm`,
      cookies: { sid },
      payload: { challenge: wireChallenge, signature },
    });
    expect(confirmResponse.statusCode).toBe(202);
    const confirmed = confirmResponse.json() as { state: string; owner: `0x${string}` };
    expect(confirmed.state).toBe("active");
  }, 30_000);

  it("a relayed command that reverts on-chain surfaces as a real HTTP error, never a fabricated 202 (ONCHAIN-07)", async () => {
    app = await buildTestApp();
    const owner = await generateVaultKeyPair();
    const wrongKey = await generateVaultKeyPair();
    const sid = await signIn(app, owner);
    const created = (await app.inject({ method: "POST", url: "/identities", cookies: { sid } })).json() as { identityId: `0x${string}` };

    const wireChallenge = (await app.inject({ method: "POST", url: `/identities/${created.identityId}/confirm-challenge`, cookies: { sid } })).json() as WireChallenge;
    // Owner's own session requests the challenge (so authorization
    // passes), but it gets signed with an unrelated key — the exact
    // signature IdentityRegistry.confirmOwnerRotation rejects on-chain.
    const badResponse = await signAndSend(app, sid, `/identities/${created.identityId}/confirm`, wireChallenge, wrongKey.privateKey);

    expect(badResponse.statusCode).not.toBe(202);
    expect(badResponse.statusCode).toBeGreaterThanOrEqual(400);

    // The identity must still read back as pending — the failed
    // attempt changed nothing.
    const read = await app.inject({ method: "GET", url: `/identities/${created.identityId}` });
    expect((read.json() as { state: string }).state).toBe("pending_confirmation");
  }, 30_000);

  it("a third party's session cannot confirm someone else's pending identity", async () => {
    app = await buildTestApp();
    const owner = await generateVaultKeyPair();
    const thirdParty = await generateVaultKeyPair();
    const ownerSid = await signIn(app, owner);
    const created = (await app.inject({ method: "POST", url: "/identities", cookies: { sid: ownerSid } })).json() as { identityId: `0x${string}` };

    const thirdPartySid = await signIn(app, thirdParty);
    const response = await app.inject({ method: "POST", url: `/identities/${created.identityId}/confirm-challenge`, cookies: { sid: thirdPartySid } });
    expect(response.statusCode).toBe(403);
  }, 30_000);

  it("creating an identity without a prior session is rejected", async () => {
    app = await buildTestApp();
    const response = await app.inject({ method: "POST", url: "/identities" });
    expect(response.statusCode).toBe(401);
  });
});

describe("Real on-chain track and account binding", () => {
  let app: Awaited<ReturnType<typeof buildTestApp>>;

  afterEach(async () => {
    await app?.close();
  });

  it("createTrack -> registerAccount -> activate -> remove, each authorized only for the identity's real owner", async () => {
    app = await buildTestApp();
    const owner = await generateVaultKeyPair();
    const sid = await signIn(app, owner);
    const created = (await app.inject({ method: "POST", url: "/identities", cookies: { sid } })).json() as { identityId: `0x${string}` };

    const confirmChallengeResponse = await app.inject({ method: "POST", url: `/identities/${created.identityId}/confirm-challenge`, cookies: { sid } });
    const confirmWire = confirmChallengeResponse.json() as WireChallenge;
    const confirmResult = await signAndSend(app, sid, `/identities/${created.identityId}/confirm`, confirmWire, owner.privateKey);
    expect(confirmResult.statusCode).toBe(202);

    const financialProfileId = keccak256(toHex("spot-nav-twr-v1"));
    const denominationCommitment = keccak256(toHex("USDT"));
    const trackChallengeResponse = await app.inject({
      method: "POST",
      url: `/identities/${created.identityId}/tracks/challenge`,
      cookies: { sid },
      payload: { financialProfileId, denominationCommitment },
    });
    expect(trackChallengeResponse.statusCode).toBe(200);
    const trackWire = trackChallengeResponse.json() as WireChallenge;
    const trackResult = await signAndSend(app, sid, `/identities/${created.identityId}/tracks`, trackWire, owner.privateKey);
    expect(trackResult.statusCode).toBe(202);
    const { trackId } = trackResult.json() as { trackId: `0x${string}` };

    const venueId = keccak256(toHex("binance"));
    const authenticatedIdCommitment = keccak256(toHex("commitment"));
    const registerChallengeResponse = await app.inject({
      method: "POST",
      url: `/tracks/${trackId}/accounts/challenge`,
      cookies: { sid },
      payload: { venueId, authenticatedIdCommitment, environment: 0 },
    });
    expect(registerChallengeResponse.statusCode).toBe(200);
    const registerWire = registerChallengeResponse.json() as WireChallenge;
    const registerResult = await signAndSend(app, sid, `/tracks/${trackId}/accounts`, registerWire, owner.privateKey);
    expect(registerResult.statusCode).toBe(202);
    const account = registerResult.json() as { accountId: `0x${string}`; state: string };
    expect(account.state).toBe("pending_baseline");

    const reasonHash = keccak256(toHex("baseline"));
    const activateChallengeResponse = await app.inject({
      method: "POST",
      url: `/accounts/${account.accountId}/activate-challenge`,
      cookies: { sid },
      payload: { reasonHash },
    });
    expect(activateChallengeResponse.statusCode).toBe(200);
    const activateWire = activateChallengeResponse.json() as WireChallenge;
    const activateResult = await signAndSend(app, sid, `/accounts/${account.accountId}/activate`, activateWire, owner.privateKey);
    expect(activateResult.statusCode).toBe(202);
    expect((activateResult.json() as { state: string }).state).toBe("active");

    const removeReasonHash = keccak256(toHex("closed"));
    const removeChallengeResponse = await app.inject({
      method: "POST",
      url: `/accounts/${account.accountId}/remove-challenge`,
      cookies: { sid },
      payload: { reasonHash: removeReasonHash },
    });
    expect(removeChallengeResponse.statusCode).toBe(200);
    const removeWire = removeChallengeResponse.json() as WireChallenge;
    const removeResult = await signAndSend(app, sid, `/accounts/${account.accountId}/remove`, removeWire, owner.privateKey);
    expect(removeResult.statusCode).toBe(202);
    expect((removeResult.json() as { state: string }).state).toBe("removed");
  }, 60_000);

  it("a third party cannot request a track-creation challenge for someone else's identity", async () => {
    app = await buildTestApp();
    const owner = await generateVaultKeyPair();
    const thirdParty = await generateVaultKeyPair();
    const sid = await signIn(app, owner);
    const created = (await app.inject({ method: "POST", url: "/identities", cookies: { sid } })).json() as { identityId: `0x${string}` };
    const confirmWire = (await app.inject({ method: "POST", url: `/identities/${created.identityId}/confirm-challenge`, cookies: { sid } })).json() as WireChallenge;
    await signAndSend(app, sid, `/identities/${created.identityId}/confirm`, confirmWire, owner.privateKey);

    const thirdPartySid = await signIn(app, thirdParty);
    const response = await app.inject({
      method: "POST",
      url: `/identities/${created.identityId}/tracks/challenge`,
      cookies: { sid: thirdPartySid },
      payload: { financialProfileId: keccak256(toHex("x")), denominationCommitment: keccak256(toHex("y")) },
    });
    expect(response.statusCode).toBe(403);
  }, 30_000);
});
