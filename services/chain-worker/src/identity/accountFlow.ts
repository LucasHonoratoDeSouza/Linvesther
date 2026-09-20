// Real createTrack/registerAccount/activateAccount/removeAccount —
// unlike identity creation, none of these need a relayer-as-temporary-
// owner step: by the time an account is bound, the identity already
// has its real owner (the user's account), so every command here is
// signed by that real owner from the start and only ever relayed, never
// authored by the relayer. Same challenge-then-relay shape throughout:
// build the exact EIP-712 payload (reading live nonce/epoch), the
// caller gets it signed, then relay the untouched signature.
import { encodeFunctionData } from "viem";
import { accountRegistryAbi, identityRegistryAbi } from "./abi.js";
import { decodeDomainEvent } from "./events.js";
import { nowDeadline, waitForSuccessfulReceipt, type RelayFlowDeps } from "./relayFlow.js";

async function identityOwnerEpoch(deps: RelayFlowDeps, identityId: `0x${string}`): Promise<bigint> {
  const [, , , ownerEpoch] = (await deps.publicClient.readContract({
    address: deps.identityRegistry,
    abi: identityRegistryAbi,
    functionName: "identities",
    args: [identityId],
  })) as readonly [`0x${string}`, `0x${string}`, bigint, bigint, bigint, number, boolean];
  return ownerEpoch;
}

async function trackIdentityId(deps: RelayFlowDeps, trackId: `0x${string}`): Promise<`0x${string}`> {
  const [identityId] = (await deps.publicClient.readContract({
    address: deps.identityRegistry,
    abi: identityRegistryAbi,
    functionName: "tracks",
    args: [trackId],
  })) as readonly [`0x${string}`, `0x${string}`, `0x${string}`, bigint, boolean];
  return identityId;
}

async function trackNonce(deps: RelayFlowDeps, trackId: `0x${string}`): Promise<bigint> {
  return (await deps.publicClient.readContract({
    address: deps.accountRegistry,
    abi: accountRegistryAbi,
    functionName: "trackNonce",
    args: [trackId],
  })) as bigint;
}

// --- createTrack (IdentityRegistry) ----------------------------------

export interface CreateTrackChallenge {
  domain: { name: string; version: string; chainId: number; verifyingContract: `0x${string}` };
  types: { CreateTrack: { name: string; type: string }[] };
  primaryType: "CreateTrack";
  message: {
    identityId: `0x${string}`;
    financialProfileId: `0x${string}`;
    denominationCommitment: `0x${string}`;
    ownerEpoch: bigint;
    nonce: bigint;
    deadline: bigint;
  };
}

export async function buildCreateTrackChallenge(
  deps: RelayFlowDeps,
  identityId: `0x${string}`,
  financialProfileId: `0x${string}`,
  denominationCommitment: `0x${string}`,
): Promise<CreateTrackChallenge> {
  const [, , , ownerEpoch, nonce] = (await deps.publicClient.readContract({
    address: deps.identityRegistry,
    abi: identityRegistryAbi,
    functionName: "identities",
    args: [identityId],
  })) as readonly [`0x${string}`, `0x${string}`, bigint, bigint, bigint, number, boolean];

  return {
    domain: { name: "LinvestherZK-IdentityRegistry", version: "1", chainId: await deps.publicClient.getChainId(), verifyingContract: deps.identityRegistry },
    types: {
      CreateTrack: [
        { name: "identityId", type: "bytes32" },
        { name: "financialProfileId", type: "bytes32" },
        { name: "denominationCommitment", type: "bytes32" },
        { name: "ownerEpoch", type: "uint64" },
        { name: "nonce", type: "uint64" },
        { name: "deadline", type: "uint256" },
      ],
    },
    primaryType: "CreateTrack",
    message: { identityId, financialProfileId, denominationCommitment, ownerEpoch, nonce, deadline: nowDeadline() },
  };
}

export async function createTrack(deps: RelayFlowDeps, challenge: CreateTrackChallenge, signature: `0x${string}`): Promise<{ trackId: `0x${string}` }> {
  const { identityId, financialProfileId, denominationCommitment, ownerEpoch, nonce, deadline } = challenge.message;
  const txHash = await deps.relayer.relay({
    dedupKey: `createTrack:${identityId}:${nonce}`,
    to: deps.identityRegistry,
    data: encodeFunctionData({
      abi: identityRegistryAbi,
      functionName: "createTrack",
      args: [identityId, financialProfileId, denominationCommitment, ownerEpoch, nonce, deadline, signature],
    }),
  });
  const receipt = await waitForSuccessfulReceipt(deps.publicClient, txHash);
  const trackCreated = receipt.logs.map((log) => decodeDomainEvent(log)).find((event) => event?.name === "TrackCreated");
  if (!trackCreated || trackCreated.name !== "TrackCreated") {
    throw new Error("createTrack transaction produced no TrackCreated event");
  }
  return { trackId: trackCreated.trackId };
}

// --- registerAccount (AccountRegistry) -------------------------------

export interface RegisterAccountChallenge {
  domain: { name: string; version: string; chainId: number; verifyingContract: `0x${string}` };
  types: { RegisterAccount: { name: string; type: string }[] };
  primaryType: "RegisterAccount";
  message: {
    trackId: `0x${string}`;
    venueId: `0x${string}`;
    authenticatedIdCommitment: `0x${string}`;
    environment: number;
    ownerEpoch: bigint;
    nonce: bigint;
    deadline: bigint;
  };
}

export async function buildRegisterAccountChallenge(
  deps: RelayFlowDeps,
  trackId: `0x${string}`,
  venueId: `0x${string}`,
  authenticatedIdCommitment: `0x${string}`,
  environment: number,
): Promise<RegisterAccountChallenge> {
  const identityId = await trackIdentityId(deps, trackId);
  const [ownerEpoch, nonce] = await Promise.all([identityOwnerEpoch(deps, identityId), trackNonce(deps, trackId)]);

  return {
    domain: { name: "LinvestherZK-AccountRegistry", version: "1", chainId: await deps.publicClient.getChainId(), verifyingContract: deps.accountRegistry },
    types: {
      RegisterAccount: [
        { name: "trackId", type: "bytes32" },
        { name: "venueId", type: "bytes32" },
        { name: "authenticatedIdCommitment", type: "bytes32" },
        { name: "environment", type: "uint8" },
        { name: "ownerEpoch", type: "uint64" },
        { name: "nonce", type: "uint64" },
        { name: "deadline", type: "uint256" },
      ],
    },
    primaryType: "RegisterAccount",
    message: { trackId, venueId, authenticatedIdCommitment, environment, ownerEpoch, nonce, deadline: nowDeadline() },
  };
}

export async function registerAccount(
  deps: RelayFlowDeps,
  challenge: RegisterAccountChallenge,
  signature: `0x${string}`,
): Promise<{ accountId: `0x${string}` }> {
  const { trackId, venueId, authenticatedIdCommitment, environment, ownerEpoch, nonce, deadline } = challenge.message;
  const txHash = await deps.relayer.relay({
    dedupKey: `registerAccount:${trackId}:${nonce}`,
    to: deps.accountRegistry,
    data: encodeFunctionData({
      abi: accountRegistryAbi,
      functionName: "registerAccount",
      args: [trackId, venueId, authenticatedIdCommitment, environment, ownerEpoch, nonce, deadline, signature],
    }),
  });
  const receipt = await waitForSuccessfulReceipt(deps.publicClient, txHash);
  const registered = receipt.logs.map((log) => decodeDomainEvent(log)).find((event) => event?.name === "AccountRegistered");
  if (!registered || registered.name !== "AccountRegistered") {
    throw new Error("registerAccount transaction produced no AccountRegistered event");
  }
  return { accountId: registered.accountId };
}

// --- activateAccount (AccountRegistry) -------------------------------

export interface ActivateAccountChallenge {
  domain: { name: string; version: string; chainId: number; verifyingContract: `0x${string}` };
  types: { ActivateAccount: { name: string; type: string }[] };
  primaryType: "ActivateAccount";
  message: { accountId: `0x${string}`; eligibleFrom: bigint; reasonHash: `0x${string}`; ownerEpoch: bigint; nonce: bigint; deadline: bigint };
}

async function accountTrackId(deps: RelayFlowDeps, accountId: `0x${string}`): Promise<`0x${string}`> {
  const [trackId] = (await deps.publicClient.readContract({
    address: deps.accountRegistry,
    abi: accountRegistryAbi,
    functionName: "accounts",
    args: [accountId],
  })) as readonly [`0x${string}`, ...unknown[]];
  return trackId;
}

export async function buildActivateAccountChallenge(
  deps: RelayFlowDeps,
  accountId: `0x${string}`,
  eligibleFrom: bigint,
  reasonHash: `0x${string}`,
): Promise<ActivateAccountChallenge> {
  const trackId = await accountTrackId(deps, accountId);
  const identityId = await trackIdentityId(deps, trackId);
  const [ownerEpoch, nonce] = await Promise.all([identityOwnerEpoch(deps, identityId), trackNonce(deps, trackId)]);

  return {
    domain: { name: "LinvestherZK-AccountRegistry", version: "1", chainId: await deps.publicClient.getChainId(), verifyingContract: deps.accountRegistry },
    types: {
      ActivateAccount: [
        { name: "accountId", type: "bytes32" },
        { name: "eligibleFrom", type: "uint64" },
        { name: "reasonHash", type: "bytes32" },
        { name: "ownerEpoch", type: "uint64" },
        { name: "nonce", type: "uint64" },
        { name: "deadline", type: "uint256" },
      ],
    },
    primaryType: "ActivateAccount",
    message: { accountId, eligibleFrom, reasonHash, ownerEpoch, nonce, deadline: nowDeadline() },
  };
}

export async function activateAccount(deps: RelayFlowDeps, challenge: ActivateAccountChallenge, signature: `0x${string}`): Promise<{ txHash: `0x${string}` }> {
  const { accountId, eligibleFrom, reasonHash, ownerEpoch, nonce, deadline } = challenge.message;
  const txHash = await deps.relayer.relay({
    dedupKey: `activateAccount:${accountId}:${nonce}`,
    to: deps.accountRegistry,
    data: encodeFunctionData({
      abi: accountRegistryAbi,
      functionName: "activateAccount",
      args: [accountId, eligibleFrom, reasonHash, ownerEpoch, nonce, deadline, signature],
    }),
  });
  await waitForSuccessfulReceipt(deps.publicClient, txHash);
  return { txHash };
}

// --- removeAccount (AccountRegistry) ----------------------------------

export interface RemoveAccountChallenge {
  domain: { name: string; version: string; chainId: number; verifyingContract: `0x${string}` };
  types: { RemoveAccount: { name: string; type: string }[] };
  primaryType: "RemoveAccount";
  message: { accountId: `0x${string}`; reasonHash: `0x${string}`; ownerEpoch: bigint; nonce: bigint; deadline: bigint };
}

export async function buildRemoveAccountChallenge(deps: RelayFlowDeps, accountId: `0x${string}`, reasonHash: `0x${string}`): Promise<RemoveAccountChallenge> {
  const trackId = await accountTrackId(deps, accountId);
  const identityId = await trackIdentityId(deps, trackId);
  const [ownerEpoch, nonce] = await Promise.all([identityOwnerEpoch(deps, identityId), trackNonce(deps, trackId)]);

  return {
    domain: { name: "LinvestherZK-AccountRegistry", version: "1", chainId: await deps.publicClient.getChainId(), verifyingContract: deps.accountRegistry },
    types: {
      RemoveAccount: [
        { name: "accountId", type: "bytes32" },
        { name: "reasonHash", type: "bytes32" },
        { name: "ownerEpoch", type: "uint64" },
        { name: "nonce", type: "uint64" },
        { name: "deadline", type: "uint256" },
      ],
    },
    primaryType: "RemoveAccount",
    message: { accountId, reasonHash, ownerEpoch, nonce, deadline: nowDeadline() },
  };
}

export async function removeAccount(deps: RelayFlowDeps, challenge: RemoveAccountChallenge, signature: `0x${string}`): Promise<{ txHash: `0x${string}` }> {
  const { accountId, reasonHash, ownerEpoch, nonce, deadline } = challenge.message;
  const txHash = await deps.relayer.relay({
    dedupKey: `removeAccount:${accountId}:${nonce}`,
    to: deps.accountRegistry,
    data: encodeFunctionData({
      abi: accountRegistryAbi,
      functionName: "removeAccount",
      args: [accountId, reasonHash, ownerEpoch, nonce, deadline, signature],
    }),
  });
  await waitForSuccessfulReceipt(deps.publicClient, txHash);
  return { txHash };
}
