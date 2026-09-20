import { createPublicClient, createWalletClient, http, keccak256, toHex } from "viem";
import { privateKeyToAccount, generatePrivateKey } from "viem/accounts";
import { Pool } from "pg";
import { afterAll, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { runMigrations } from "../../src/migrate.js";
import { PostgresProjectionStore } from "../../src/identity/postgresProjectionStore.js";
import { identityRegistryAbi } from "../../src/identity/abi.js";
import { Indexer } from "../../src/indexer/indexer.js";
import { ViemChainClient } from "../../src/indexer/viemClient.js";
import {
  DEPLOYER_KEY,
  signIdentityRegistryCommand,
  startTestChain,
  stopTestChain,
  type TestChain,
} from "./testChain.js";

async function anvilRpc(rpcUrl: string, method: string, params: unknown[]): Promise<unknown> {
  const response = await fetch(rpcUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  const body = (await response.json()) as { result?: unknown; error?: { message: string } };
  if (body.error) {
    throw new Error(`${method} failed: ${body.error.message}`);
  }
  return body.result;
}

const PORT = 8800;
const DATABASE_URL = process.env.DATABASE_URL ?? "postgresql://linvestherzk:linvestherzk-local-dev-only@localhost:5433/linvestherzk";
const deadline = 9_999_999_999n;

let chain: TestChain;
let pool: Pool;

beforeAll(async () => {
  chain = await startTestChain(PORT);
  pool = new Pool({ connectionString: DATABASE_URL });
  await pool.query("DROP TABLE IF EXISTS accounts, tracks, identities, indexed_blocks, schema_migrations CASCADE");
  await runMigrations(pool);
}, 30_000);

beforeEach(async () => {
  await pool.query("TRUNCATE accounts, tracks, identities, indexed_blocks");
});

afterAll(async () => {
  stopTestChain(chain);
  await pool.end();
});

function newStore(): PostgresProjectionStore {
  const publicClient = createPublicClient({ transport: http(chain.rpcUrl) });
  return new PostgresProjectionStore(pool, publicClient, [chain.identityRegistry, chain.accountRegistry]);
}

describe("PostgresProjectionStore against real Anvil + real Postgres", () => {
  it("applies a real IdentityCreated block, is readable, and survives a fresh store instance (ONCHAIN-09)", async () => {
    const store = newStore();
    const publicClient = createPublicClient({ transport: http(chain.rpcUrl) });
    const deployerAccount = privateKeyToAccount(DEPLOYER_KEY);
    const wallet = createWalletClient({ account: deployerAccount, transport: http(chain.rpcUrl) });

    const hash = await wallet.writeContract({
      chain: undefined,
      address: chain.identityRegistry,
      abi: identityRegistryAbi,
      functionName: "createIdentity",
      args: [0],
    });
    const receipt = await publicClient.waitForTransactionReceipt({ hash });
    const block = await publicClient.getBlock({ blockNumber: receipt.blockNumber });

    await store.apply({
      header: { number: receipt.blockNumber, hash: block.hash!, parentHash: block.parentHash },
      tag: "included",
    });

    const identityId = receipt.logs[0]!.topics[1] as `0x${string}`;
    const identity = await store.getIdentity(identityId);
    expect(identity).toMatchObject({ owner: deployerAccount.address, state: "pending_confirmation", ownerEpoch: 0n });

    // A fresh instance (new object, same Postgres) must read the same
    // row — nothing lived only in this store object's own memory.
    const restarted = newStore();
    const identityAfterRestart = await restarted.getIdentity(identityId);
    expect(identityAfterRestart).toEqual(identity);
  }, 30_000);

  it("full create -> propose -> confirm flow ends with the new owner active, read back correctly", async () => {
    const store = newStore();
    const publicClient = createPublicClient({ transport: http(chain.rpcUrl) });
    const deployerAccount = privateKeyToAccount(DEPLOYER_KEY);
    const wallet = createWalletClient({ account: deployerAccount, transport: http(chain.rpcUrl) });
    const newOwnerKey = generatePrivateKey();
    const newOwnerAccount = privateKeyToAccount(newOwnerKey);

    async function applyLatestBlock(txHash: `0x${string}`) {
      const receipt = await publicClient.waitForTransactionReceipt({ hash: txHash });
      const block = await publicClient.getBlock({ blockNumber: receipt.blockNumber });
      await store.apply({ header: { number: receipt.blockNumber, hash: block.hash!, parentHash: block.parentHash }, tag: "included" });
      return receipt;
    }

    const createHash = await wallet.writeContract({
      chain: undefined,
      address: chain.identityRegistry,
      abi: identityRegistryAbi,
      functionName: "createIdentity",
      args: [0],
    });
    const createReceipt = await applyLatestBlock(createHash);
    const identityId = createReceipt.logs[0]!.topics[1] as `0x${string}`;

    const proposeSignature = await signIdentityRegistryCommand(chain, DEPLOYER_KEY, "ProposeOwnerRotation", {
      identityId,
      newOwner: newOwnerAccount.address,
      ownerEpoch: 0n,
      nonce: 0n,
      deadline,
    });
    const proposeHash = await wallet.writeContract({
      chain: undefined,
      address: chain.identityRegistry,
      abi: identityRegistryAbi,
      functionName: "proposeOwnerRotation",
      args: [identityId, newOwnerAccount.address, 0n, 0n, deadline, proposeSignature],
    });
    await applyLatestBlock(proposeHash);

    const pendingIdentity = await store.getIdentity(identityId);
    expect(pendingIdentity).toMatchObject({ owner: deployerAccount.address, pendingOwner: newOwnerAccount.address, state: "pending_confirmation" });

    const confirmSignature = await signIdentityRegistryCommand(chain, newOwnerKey, "ConfirmOwnerRotation", {
      identityId,
      newOwner: newOwnerAccount.address,
      ownerEpoch: 0n,
      nonce: 1n,
      deadline,
    });
    const confirmHash = await wallet.writeContract({
      chain: undefined,
      address: chain.identityRegistry,
      abi: identityRegistryAbi,
      functionName: "confirmOwnerRotation",
      args: [identityId, newOwnerAccount.address, 0n, 1n, deadline, confirmSignature],
    });
    await applyLatestBlock(confirmHash);

    const activeIdentity = await store.getIdentity(identityId);
    expect(activeIdentity).toMatchObject({ owner: newOwnerAccount.address, pendingOwner: null, ownerEpoch: 1n, state: "active" });
  }, 30_000);

  it("revert undoes a previously applied block's projection", async () => {
    const store = newStore();
    const publicClient = createPublicClient({ transport: http(chain.rpcUrl) });
    const deployerAccount = privateKeyToAccount(DEPLOYER_KEY);
    const wallet = createWalletClient({ account: deployerAccount, transport: http(chain.rpcUrl) });

    const hash = await wallet.writeContract({
      chain: undefined,
      address: chain.identityRegistry,
      abi: identityRegistryAbi,
      functionName: "createIdentity",
      args: [0],
    });
    const receipt = await publicClient.waitForTransactionReceipt({ hash });
    const block = await publicClient.getBlock({ blockNumber: receipt.blockNumber });
    const indexedBlock = { header: { number: receipt.blockNumber, hash: block.hash!, parentHash: block.parentHash }, tag: "included" as const };

    await store.apply(indexedBlock);
    const identityId = receipt.logs[0]!.topics[1] as `0x${string}`;
    expect(await store.getIdentity(identityId)).not.toBeNull();

    await store.revert(indexedBlock);
    expect(await store.getIdentity(identityId)).toBeNull();
  }, 30_000);

  it("a real anvil_reorg, driven through the Indexer's own reorg-detection path, undoes an identity created only on the abandoned branch (ONCHAIN-10)", async () => {
    const store = newStore();
    const indexer = new Indexer(store);
    const chainClient = new ViemChainClient(chain.rpcUrl);
    const publicClient = createPublicClient({ transport: http(chain.rpcUrl) });
    const deployerAccount = privateKeyToAccount(DEPLOYER_KEY);
    const wallet = createWalletClient({ account: deployerAccount, transport: http(chain.rpcUrl) });

    // A few empty blocks first, so the identity-creating block isn't
    // the chain tip at height 0/1 — gives the reorg real depth to work
    // with, the same setup indexer/anvil.integration.test.ts uses.
    await anvilRpc(chain.rpcUrl, "anvil_mine", ["0x2"]);
    await indexer.sync(chainClient, 0n);

    const hash = await wallet.writeContract({
      chain: undefined,
      address: chain.identityRegistry,
      abi: identityRegistryAbi,
      functionName: "createIdentity",
      args: [0],
    });
    const receipt = await publicClient.waitForTransactionReceipt({ hash });
    const identityId = receipt.logs[0]!.topics[1] as `0x${string}`;

    await indexer.sync(chainClient, 0n);
    expect(await store.getIdentity(identityId)).not.toBeNull();

    // Real reorg: replaces the top blocks (including the one the
    // identity was created in) with freshly mined, empty ones — the
    // createIdentity transaction is gone from the canonical chain.
    await anvilRpc(chain.rpcUrl, "anvil_reorg", [2, []]);
    const report = await indexer.sync(chainClient, 0n);

    expect(report).not.toBeNull();
    expect(await store.getIdentity(identityId)).toBeNull();
  }, 30_000);
});
