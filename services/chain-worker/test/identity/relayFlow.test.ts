import { createPublicClient, createWalletClient, http, hashTypedData } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { MemoryRelayStore } from "../../src/relayer/memoryRelayStore.js";
import { Relayer } from "../../src/relayer/relayer.js";
import { ViemBroadcastClient } from "../../src/relayer/viemBroadcastClient.js";
import { identityRegistryAbi } from "../../src/identity/abi.js";
import { buildConfirmOwnerRotationChallenge, confirmIdentityCreation, startIdentityCreation, type RelayFlowDeps } from "../../src/identity/relayFlow.js";
import { DEPLOYER_KEY, generateVaultKey, signVaultDigest, startTestChain, stopTestChain, type TestChain } from "./testChain.js";

const PORT = 8801;

let chain: TestChain;
let deps: RelayFlowDeps;

beforeAll(async () => {
  chain = await startTestChain(PORT);
  const relayerAccount = privateKeyToAccount(DEPLOYER_KEY);
  const broadcastClient = new ViemBroadcastClient(chain.rpcUrl, DEPLOYER_KEY);
  const relayer = new Relayer(broadcastClient, new MemoryRelayStore());
  const publicClient = createPublicClient({ transport: http(chain.rpcUrl) });
  deps = {
    relayer,
    relayerAccount,
    publicClient,
    identityRegistry: chain.identityRegistry,
    accountFactory: chain.accountFactory,
    accountRegistry: chain.accountRegistry,
  };
}, 30_000);

afterAll(() => {
  stopTestChain(chain);
});

describe("relayFlow against real Anvil", () => {
  it("startIdentityCreation deploys the account, creates the identity, and proposes the account as owner (ONCHAIN-04, ONCHAIN-05)", async () => {
    const key = await generateVaultKey();
    const result = await startIdentityCreation(deps, key.qx, key.qy, "vault");

    expect(result.accountAddress).toMatch(/^0x[0-9a-fA-F]{40}$/);
    const code = await deps.publicClient.getCode({ address: result.accountAddress });
    expect(code).not.toBe("0x");

    const [owner, pendingOwner] = (await deps.publicClient.readContract({
      address: chain.identityRegistry,
      abi: identityRegistryAbi,
      functionName: "identities",
      args: [result.identityId],
    })) as readonly [`0x${string}`, `0x${string}`, bigint, bigint, bigint, number, boolean];

    expect(owner.toLowerCase()).toBe(privateKeyToAccount(DEPLOYER_KEY).address.toLowerCase());
    expect(pendingOwner.toLowerCase()).toBe(result.accountAddress.toLowerCase());
  }, 30_000);

  it("confirmIdentityCreation, signed by the real vault key, transfers ownership to the account (ONCHAIN-06)", async () => {
    const key = await generateVaultKey();
    const { identityId, accountAddress } = await startIdentityCreation(deps, key.qx, key.qy, "vault");

    const challenge = await buildConfirmOwnerRotationChallenge(deps, identityId, accountAddress);
    const digest = hashTypedData(challenge);
    const signature = await signVaultDigest(key.privateKey, digest);

    await confirmIdentityCreation(deps, challenge, signature);

    const [owner, pendingOwner, , ownerEpoch] = (await deps.publicClient.readContract({
      address: chain.identityRegistry,
      abi: identityRegistryAbi,
      functionName: "identities",
      args: [identityId],
    })) as readonly [`0x${string}`, `0x${string}`, bigint, bigint, bigint, number, boolean];

    expect(owner.toLowerCase()).toBe(accountAddress.toLowerCase());
    expect(pendingOwner).toBe("0x0000000000000000000000000000000000000000");
    expect(ownerEpoch).toBe(1n);
  }, 30_000);

  it("a signature that makes the on-chain command revert surfaces as a real error, never a fabricated success (ONCHAIN-07)", async () => {
    const key = await generateVaultKey();
    const wrongKey = await generateVaultKey();
    const { identityId, accountAddress } = await startIdentityCreation(deps, key.qx, key.qy, "vault");

    const challenge = await buildConfirmOwnerRotationChallenge(deps, identityId, accountAddress);
    const digest = hashTypedData(challenge);
    // Syntactically a valid signature, but from a key that doesn't
    // control accountAddress — IdentityRegistry.confirmOwnerRotation
    // reverts with InvalidSignature, a real on-chain failure.
    const wrongSignature = await signVaultDigest(wrongKey.privateKey, digest);

    // Anvil (like most nodes) simulates a transaction as part of
    // eth_sendTransaction, so an obviously-reverting call is rejected
    // right there — the real failure mode observed here — rather than
    // being mined and only then found reverted (the case
    // waitForSuccessfulReceipt's explicit status check exists for, on
    // a node/scenario that doesn't pre-simulate). Both are real,
    // thrown errors; neither is a fabricated success either way.
    await expect(confirmIdentityCreation(deps, challenge, wrongSignature)).rejects.toThrow(/revert/i);

    // The identity must still show the pre-confirmation state — the
    // reverted attempt changed nothing.
    const [owner, pendingOwner] = (await deps.publicClient.readContract({
      address: chain.identityRegistry,
      abi: identityRegistryAbi,
      functionName: "identities",
      args: [identityId],
    })) as readonly [`0x${string}`, `0x${string}`, bigint, bigint, bigint, number, boolean];
    expect(owner.toLowerCase()).toBe(privateKeyToAccount(DEPLOYER_KEY).address.toLowerCase());
    expect(pendingOwner.toLowerCase()).toBe(accountAddress.toLowerCase());
  }, 30_000);

  it("a retried startIdentityCreation call for the same key reuses the same account and identity, not new ones (dedupKey idempotency)", async () => {
    // Relayer.relay()'s dedupKey guarantee is retry-idempotency for
    // sequential calls (recover the existing tx/nonce rather than
    // broadcast a new one) — the same contract a client library would
    // rely on after e.g. a timed-out request, not lock-free safety
    // under truly simultaneous concurrent calls (createIdentity() has
    // no on-chain idempotency key of its own to dedupe two genuinely
    // racing broadcasts against).
    const key = await generateVaultKey();

    const first = await startIdentityCreation(deps, key.qx, key.qy, "vault");
    const second = await startIdentityCreation(deps, key.qx, key.qy, "vault");

    expect(first.accountAddress).toBe(second.accountAddress);
    expect(first.identityId).toBe(second.identityId);
  }, 30_000);
});
