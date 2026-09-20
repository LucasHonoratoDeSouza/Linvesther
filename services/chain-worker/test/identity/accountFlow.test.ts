import { createPublicClient, http, hashTypedData, keccak256, toHex } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { MemoryRelayStore } from "../../src/relayer/memoryRelayStore.js";
import { Relayer } from "../../src/relayer/relayer.js";
import { ViemBroadcastClient } from "../../src/relayer/viemBroadcastClient.js";
import { accountRegistryAbi } from "../../src/identity/abi.js";
import { confirmIdentityCreation, buildConfirmOwnerRotationChallenge, startIdentityCreation, type RelayFlowDeps } from "../../src/identity/relayFlow.js";
import {
  activateAccount,
  buildActivateAccountChallenge,
  buildCreateTrackChallenge,
  buildRegisterAccountChallenge,
  buildRemoveAccountChallenge,
  createTrack,
  registerAccount,
  removeAccount,
} from "../../src/identity/accountFlow.js";
import { DEPLOYER_KEY, generateVaultKey, signVaultDigest, startTestChain, stopTestChain, type TestChain, type TestVaultKey } from "./testChain.js";

const PORT = 8802;

let chain: TestChain;
let deps: RelayFlowDeps;

async function createAndConfirmIdentity(key: TestVaultKey) {
  const { identityId, accountAddress } = await startIdentityCreation(deps, key.qx, key.qy, "vault");
  const challenge = await buildConfirmOwnerRotationChallenge(deps, identityId, accountAddress);
  const digest = hashTypedData(challenge);
  const signature = await signVaultDigest(key.privateKey, digest);
  await confirmIdentityCreation(deps, challenge, signature);
  return { identityId, accountAddress };
}

beforeAll(async () => {
  chain = await startTestChain(PORT);
  const relayerAccount = privateKeyToAccount(DEPLOYER_KEY);
  const relayer = new Relayer(new ViemBroadcastClient(chain.rpcUrl, DEPLOYER_KEY), new MemoryRelayStore());
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

describe("accountFlow against real Anvil", () => {
  it("createTrack -> registerAccount -> activateAccount -> removeAccount, each signed by the real account owner", async () => {
    const key = await generateVaultKey();
    const { identityId } = await createAndConfirmIdentity(key);

    const financialProfileId = keccak256(toHex("spot-nav-twr-v1"));
    const denominationCommitment = keccak256(toHex("USDT"));
    const trackChallenge = await buildCreateTrackChallenge(deps, identityId, financialProfileId, denominationCommitment);
    expect(trackChallenge.message.ownerEpoch).toBe(1n); // set by confirmOwnerRotation above
    const trackDigest = hashTypedData(trackChallenge);
    const trackSignature = await signVaultDigest(key.privateKey, trackDigest);
    const { trackId } = await createTrack(deps, trackChallenge, trackSignature);
    expect(trackId).toMatch(/^0x[0-9a-fA-F]{64}$/);

    const venueId = keccak256(toHex("binance"));
    const authenticatedIdCommitment = keccak256(toHex("commitment"));
    const registerChallenge = await buildRegisterAccountChallenge(deps, trackId, venueId, authenticatedIdCommitment, 0);
    const registerDigest = hashTypedData(registerChallenge);
    const registerSignature = await signVaultDigest(key.privateKey, registerDigest);
    const { accountId } = await registerAccount(deps, registerChallenge, registerSignature);
    expect(accountId).toMatch(/^0x[0-9a-fA-F]{64}$/);

    // AccountBinding's field order (contracts/src/AccountRegistry.sol):
    // trackId, venueId, authenticatedIdCommitment, environment,
    // registeredAt, eligibleFrom, removedAt, bindingEpoch, state, exists.
    const accountBefore = (await deps.publicClient.readContract({
      address: chain.accountRegistry,
      abi: accountRegistryAbi,
      functionName: "accounts",
      args: [accountId],
    })) as readonly [`0x${string}`, `0x${string}`, `0x${string}`, number, bigint, bigint, bigint, bigint, number, boolean];
    const registeredAt = accountBefore[4];
    expect(accountBefore[8]).toBe(0); // AccountState.PendingBaseline

    const reasonHash = keccak256(toHex("baseline"));
    const activateChallenge = await buildActivateAccountChallenge(deps, accountId, registeredAt, reasonHash);
    const activateDigest = hashTypedData(activateChallenge);
    const activateSignature = await signVaultDigest(key.privateKey, activateDigest);
    await activateAccount(deps, activateChallenge, activateSignature);

    const accountAfterActivate = (await deps.publicClient.readContract({
      address: chain.accountRegistry,
      abi: accountRegistryAbi,
      functionName: "accounts",
      args: [accountId],
    })) as readonly [`0x${string}`, `0x${string}`, `0x${string}`, number, bigint, bigint, bigint, bigint, number, boolean];
    expect(accountAfterActivate[8]).toBe(1); // AccountState.Active

    const removeReasonHash = keccak256(toHex("closed"));
    const removeChallenge = await buildRemoveAccountChallenge(deps, accountId, removeReasonHash);
    const removeDigest = hashTypedData(removeChallenge);
    const removeSignature = await signVaultDigest(key.privateKey, removeDigest);
    await removeAccount(deps, removeChallenge, removeSignature);

    const accountAfterRemove = (await deps.publicClient.readContract({
      address: chain.accountRegistry,
      abi: accountRegistryAbi,
      functionName: "accounts",
      args: [accountId],
    })) as readonly [`0x${string}`, `0x${string}`, `0x${string}`, number, bigint, bigint, bigint, bigint, number, boolean];
    expect(accountAfterRemove[8]).toBe(2); // AccountState.Removed
  }, 30_000);
});
