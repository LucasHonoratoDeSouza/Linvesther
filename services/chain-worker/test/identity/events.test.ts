import { createPublicClient, createWalletClient, http, keccak256, toHex, type Log } from "viem";
import { privateKeyToAccount, generatePrivateKey } from "viem/accounts";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { accountRegistryAbi, identityRegistryAbi } from "../../src/identity/abi.js";
import { decodeDomainEvent } from "../../src/identity/events.js";
import {
  DEPLOYER_KEY,
  signAccountRegistryCommand,
  signIdentityRegistryCommand,
  startTestChain,
  stopTestChain,
  type TestChain,
} from "./testChain.js";

const PORT = 8799;
const deadline = 9_999_999_999n;

let chain: TestChain;

beforeAll(async () => {
  chain = await startTestChain(PORT);
}, 30_000);

afterAll(() => {
  stopTestChain(chain);
});

describe("decodeDomainEvent against real Anvil-generated logs", () => {
  it("decodes every real IdentityRegistry/AccountRegistry event correctly, and returns null for an unrelated log", async () => {
    const publicClient = createPublicClient({ transport: http(chain.rpcUrl) });
    const deployerAccount = privateKeyToAccount(DEPLOYER_KEY);
    const wallet = createWalletClient({ account: deployerAccount, transport: http(chain.rpcUrl) });
    const newOwnerKey = generatePrivateKey();
    const newOwnerAccount = privateKeyToAccount(newOwnerKey);

    // 1. createIdentity — no signature required (self-registration).
    const createIdentityHash = await wallet.writeContract({
      chain: undefined,
      address: chain.identityRegistry,
      abi: identityRegistryAbi,
      functionName: "createIdentity",
      args: [0],
    });
    const createIdentityReceipt = await publicClient.waitForTransactionReceipt({ hash: createIdentityHash });
    const identityCreatedLog = createIdentityReceipt.logs.find((log) => decodeDomainEvent(log)?.name === "IdentityCreated");
    expect(identityCreatedLog).toBeDefined();
    const identityCreated = decodeDomainEvent(identityCreatedLog as Log);
    expect(identityCreated).toMatchObject({ name: "IdentityCreated", owner: deployerAccount.address, identityType: 0 });
    const identityId = (identityCreated as { identityId: `0x${string}` }).identityId;

    // 2. createTrack — signed by the current owner (the deployer).
    const financialProfileId = keccak256(toHex("financial-profile"));
    const denominationCommitment = keccak256(toHex("denomination"));
    const createTrackSignature = await signIdentityRegistryCommand(chain, DEPLOYER_KEY, "CreateTrack", {
      identityId,
      financialProfileId,
      denominationCommitment,
      ownerEpoch: 0n,
      nonce: 0n,
      deadline,
    });
    const createTrackHash = await wallet.writeContract({
      chain: undefined,
      address: chain.identityRegistry,
      abi: identityRegistryAbi,
      functionName: "createTrack",
      args: [identityId, financialProfileId, denominationCommitment, 0n, 0n, deadline, createTrackSignature],
    });
    const createTrackReceipt = await publicClient.waitForTransactionReceipt({ hash: createTrackHash });
    const trackCreatedLog = createTrackReceipt.logs.find((log) => decodeDomainEvent(log)?.name === "TrackCreated");
    const trackCreated = decodeDomainEvent(trackCreatedLog as Log);
    expect(trackCreated).toMatchObject({ name: "TrackCreated", identityId, financialProfileId, denominationCommitment });
    const trackId = (trackCreated as { trackId: `0x${string}` }).trackId;

    // 3. proposeOwnerRotation — signed by the current owner.
    const proposeSignature = await signIdentityRegistryCommand(chain, DEPLOYER_KEY, "ProposeOwnerRotation", {
      identityId,
      newOwner: newOwnerAccount.address,
      ownerEpoch: 0n,
      nonce: 1n,
      deadline,
    });
    const proposeHash = await wallet.writeContract({
      chain: undefined,
      address: chain.identityRegistry,
      abi: identityRegistryAbi,
      functionName: "proposeOwnerRotation",
      args: [identityId, newOwnerAccount.address, 0n, 1n, deadline, proposeSignature],
    });
    const proposeReceipt = await publicClient.waitForTransactionReceipt({ hash: proposeHash });
    const proposedLog = proposeReceipt.logs.find((log) => decodeDomainEvent(log)?.name === "OwnerRotationProposed");
    expect(decodeDomainEvent(proposedLog as Log)).toMatchObject({
      name: "OwnerRotationProposed",
      identityId,
      currentOwner: deployerAccount.address,
      pendingOwner: newOwnerAccount.address,
    });

    // 4. confirmOwnerRotation — signed by the NEW owner.
    const confirmSignature = await signIdentityRegistryCommand(chain, newOwnerKey, "ConfirmOwnerRotation", {
      identityId,
      newOwner: newOwnerAccount.address,
      ownerEpoch: 0n,
      nonce: 2n,
      deadline,
    });
    const confirmHash = await wallet.writeContract({
      chain: undefined,
      address: chain.identityRegistry,
      abi: identityRegistryAbi,
      functionName: "confirmOwnerRotation",
      args: [identityId, newOwnerAccount.address, 0n, 2n, deadline, confirmSignature],
    });
    const confirmReceipt = await publicClient.waitForTransactionReceipt({ hash: confirmHash });
    const rotatedLog = confirmReceipt.logs.find((log) => decodeDomainEvent(log)?.name === "OwnerRotated");
    expect(decodeDomainEvent(rotatedLog as Log)).toMatchObject({
      name: "OwnerRotated",
      identityId,
      previousOwner: deployerAccount.address,
      newOwner: newOwnerAccount.address,
      newOwnerEpoch: 1n,
    });

    // 5. registerAccount/activateAccount/removeAccount — signed by the
    // track's owner, now newOwnerAccount after the rotation above.
    const venueId = keccak256(toHex("binance"));
    const authenticatedIdCommitment = keccak256(toHex("commitment"));
    const registerSignature = await signAccountRegistryCommand(chain, newOwnerKey, "RegisterAccount", {
      trackId,
      venueId,
      authenticatedIdCommitment,
      environment: 0,
      ownerEpoch: 1n,
      nonce: 0n,
      deadline,
    });
    const registerHash = await wallet.writeContract({
      chain: undefined,
      address: chain.accountRegistry,
      abi: accountRegistryAbi,
      functionName: "registerAccount",
      args: [trackId, venueId, authenticatedIdCommitment, 0, 1n, 0n, deadline, registerSignature],
    });
    const registerReceipt = await publicClient.waitForTransactionReceipt({ hash: registerHash });
    const registeredLog = registerReceipt.logs.find((log) => decodeDomainEvent(log)?.name === "AccountRegistered");
    const registered = decodeDomainEvent(registeredLog as Log);
    expect(registered).toMatchObject({ name: "AccountRegistered", trackId, venueId });
    const accountId = (registered as { accountId: `0x${string}` }).accountId;

    const registerBlock = await publicClient.getBlock({ blockNumber: registerReceipt.blockNumber });
    const eligibleFrom = registerBlock.timestamp;
    const reasonHash = keccak256(toHex("baseline"));
    const activateSignature = await signAccountRegistryCommand(chain, newOwnerKey, "ActivateAccount", {
      accountId,
      eligibleFrom,
      reasonHash,
      ownerEpoch: 1n,
      nonce: 1n,
      deadline,
    });
    const activateHash = await wallet.writeContract({
      chain: undefined,
      address: chain.accountRegistry,
      abi: accountRegistryAbi,
      functionName: "activateAccount",
      args: [accountId, eligibleFrom, reasonHash, 1n, 1n, deadline, activateSignature],
    });
    const activateReceipt = await publicClient.waitForTransactionReceipt({ hash: activateHash });
    const activatedLog = activateReceipt.logs.find((log) => decodeDomainEvent(log)?.name === "AccountActivated");
    expect(decodeDomainEvent(activatedLog as Log)).toMatchObject({ name: "AccountActivated", trackId, accountId });

    const removeReasonHash = keccak256(toHex("closed"));
    const removeSignature = await signAccountRegistryCommand(chain, newOwnerKey, "RemoveAccount", {
      accountId,
      reasonHash: removeReasonHash,
      ownerEpoch: 1n,
      nonce: 2n,
      deadline,
    });
    const removeHash = await wallet.writeContract({
      chain: undefined,
      address: chain.accountRegistry,
      abi: accountRegistryAbi,
      functionName: "removeAccount",
      args: [accountId, removeReasonHash, 1n, 2n, deadline, removeSignature],
    });
    const removeReceipt = await publicClient.waitForTransactionReceipt({ hash: removeHash });
    const removedLog = removeReceipt.logs.find((log) => decodeDomainEvent(log)?.name === "AccountRemoved");
    expect(decodeDomainEvent(removedLog as Log)).toMatchObject({ name: "AccountRemoved", trackId, accountId });

    // 6. A log from neither contract's ABI never decodes as something
    // else — e.g. the EIP712DomainChanged log both contracts also emit
    // at construction is real but has no domain meaning, so it must
    // come back null, not misattributed to one of the events above.
    const domainChangedLog = createIdentityReceipt.logs.find((log) => log.address.toLowerCase() === chain.identityRegistry.toLowerCase() && log.topics[0] !== identityCreatedLog!.topics[0]);
    if (domainChangedLog) {
      expect(decodeDomainEvent(domainChangedLog)).toBeNull();
    }
    const garbageLog: Log = { ...(identityCreatedLog as Log), topics: ["0x" + "ab".repeat(32)] as [`0x${string}`] };
    expect(decodeDomainEvent(garbageLog)).toBeNull();
  }, 30_000);
});
