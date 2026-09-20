// Orchestrates the real create -> propose -> confirm sequence
// (ONCHAIN-04..06): the relayer becomes an identity's owner only
// because IdentityRegistry.createIdentity() has no other way to set an
// initial owner (self-registration, per its own NatSpec), then
// immediately proposes the user's smart account as the real owner. The
// relayer's authority ends the moment the user's own signature confirms
// the rotation — nothing here gives the relayer power beyond that.
import { type PublicClient, type TransactionReceipt, encodeFunctionData } from "viem";
import type { Account } from "viem/accounts";
import type { Relayer } from "../relayer/relayer.js";
import { accountFactoryAbi, identityRegistryAbi } from "./abi.js";
import { decodeDomainEvent } from "./events.js";

export const DEADLINE_SECONDS = 600n; // 10 minutes — long enough for a relay retry, short enough that a stale signature can't be replayed much later.

/** A relayed command was mined but reverted (e.g. a stale/invalid
 * signature, an expired deadline) — a real on-chain failure, never
 * silently treated as success (ONCHAIN-07). `waitForTransactionReceipt`
 * alone doesn't throw on revert (the tx is real and mined, just
 * unsuccessful), so every call site here checks `status` explicitly. */
export class RelayTransactionRevertedError extends Error {
  constructor(public readonly txHash: `0x${string}`) {
    super(`relayed transaction ${txHash} reverted`);
  }
}

export async function waitForSuccessfulReceipt(publicClient: PublicClient, hash: `0x${string}`): Promise<TransactionReceipt> {
  const receipt = await publicClient.waitForTransactionReceipt({ hash });
  if (receipt.status === "reverted") {
    throw new RelayTransactionRevertedError(hash);
  }
  return receipt;
}

export interface RelayFlowDeps {
  relayer: Relayer;
  relayerAccount: Account;
  publicClient: PublicClient;
  identityRegistry: `0x${string}`;
  accountFactory: `0x${string}`;
  accountRegistry: `0x${string}`;
}

export interface StartIdentityCreationResult {
  identityId: `0x${string}`;
  accountAddress: `0x${string}`;
}

export function nowDeadline(): bigint {
  return BigInt(Math.floor(Date.now() / 1000)) + DEADLINE_SECONDS;
}

async function ensureAccountDeployed(
  deps: RelayFlowDeps,
  method: "webauthn" | "vault",
  qx: `0x${string}`,
  qy: `0x${string}`,
): Promise<`0x${string}`> {
  const predictFn = method === "webauthn" ? "predictWebAuthnAccountAddress" : "predictP256VaultAccountAddress";
  const deployFn = method === "webauthn" ? "deployWebAuthnAccount" : "deployP256VaultAccount";

  const predicted = (await deps.publicClient.readContract({
    address: deps.accountFactory,
    abi: accountFactoryAbi,
    functionName: predictFn,
    args: [qx, qy],
  })) as `0x${string}`;

  const code = await deps.publicClient.getCode({ address: predicted });
  if (code && code !== "0x") {
    return predicted;
  }

  const deployTxHash = await deps.relayer.relay({
    dedupKey: `deploy:${method}:${qx}:${qy}`,
    to: deps.accountFactory,
    data: encodeFunctionData({ abi: accountFactoryAbi, functionName: deployFn, args: [qx, qy] }),
  });
  await waitForSuccessfulReceipt(deps.publicClient, deployTxHash);
  return predicted;
}

/** Deploys the user's account (if needed), creates the identity (the
 * relayer becomes its temporary owner), then proposes the user's
 * account as the real owner. Returns as soon as the propose step is
 * on-chain — the identity stays `pending_confirmation` (per the
 * indexer's projection) until `confirmIdentityCreation` runs. */
export async function startIdentityCreation(
  deps: RelayFlowDeps,
  qx: `0x${string}`,
  qy: `0x${string}`,
  method: "webauthn" | "vault",
): Promise<StartIdentityCreationResult> {
  const accountAddress = await ensureAccountDeployed(deps, method, qx, qy);

  const createTxHash = await deps.relayer.relay({
    dedupKey: `create:${method}:${qx}:${qy}`,
    to: deps.identityRegistry,
    data: encodeFunctionData({ abi: identityRegistryAbi, functionName: "createIdentity", args: [0] }),
  });
  const createReceipt = await waitForSuccessfulReceipt(deps.publicClient, createTxHash);
  const identityCreated = createReceipt.logs.map((log) => decodeDomainEvent(log)).find((event) => event?.name === "IdentityCreated");
  if (!identityCreated || identityCreated.name !== "IdentityCreated") {
    throw new Error("createIdentity transaction produced no IdentityCreated event");
  }
  const identityId = identityCreated.identityId;

  const deadline = nowDeadline();
  const proposeSignature = await deps.relayerAccount.signTypedData!({
    domain: { name: "LinvestherZK-IdentityRegistry", version: "1", chainId: await deps.publicClient.getChainId(), verifyingContract: deps.identityRegistry },
    types: {
      ProposeOwnerRotation: [
        { name: "identityId", type: "bytes32" },
        { name: "newOwner", type: "address" },
        { name: "ownerEpoch", type: "uint64" },
        { name: "nonce", type: "uint64" },
        { name: "deadline", type: "uint256" },
      ],
    },
    primaryType: "ProposeOwnerRotation",
    message: { identityId, newOwner: accountAddress, ownerEpoch: 0n, nonce: 0n, deadline },
  });

  const proposeTxHash = await deps.relayer.relay({
    dedupKey: `propose:${identityId}`,
    to: deps.identityRegistry,
    data: encodeFunctionData({
      abi: identityRegistryAbi,
      functionName: "proposeOwnerRotation",
      args: [identityId, accountAddress, 0n, 0n, deadline, proposeSignature],
    }),
  });
  // Relayer.relay() returns as soon as the tx is broadcast, not mined —
  // buildConfirmOwnerRotationChallenge reads the contract's live nonce
  // right after this returns, so that read must not race the propose
  // transaction actually landing.
  await waitForSuccessfulReceipt(deps.publicClient, proposeTxHash);

  return { identityId, accountAddress };
}

export interface ConfirmOwnerRotationChallenge {
  domain: { name: string; version: string; chainId: number; verifyingContract: `0x${string}` };
  types: { ConfirmOwnerRotation: { name: string; type: string }[] };
  primaryType: "ConfirmOwnerRotation";
  message: { identityId: `0x${string}`; newOwner: `0x${string}`; ownerEpoch: bigint; nonce: bigint; deadline: bigint };
}

/** Builds the exact EIP-712 payload the user's browser must sign to
 * confirm ownership — reads `ownerEpoch`/`nonce` live from the
 * contract, since those are the values the resulting signature has to
 * cover. `confirmIdentityCreation` later relays a signature over
 * exactly this `message`, unmodified. */
export async function buildConfirmOwnerRotationChallenge(
  deps: RelayFlowDeps,
  identityId: `0x${string}`,
  newOwner: `0x${string}`,
): Promise<ConfirmOwnerRotationChallenge> {
  const [, , , ownerEpoch, nonce] = (await deps.publicClient.readContract({
    address: deps.identityRegistry,
    abi: identityRegistryAbi,
    functionName: "identities",
    args: [identityId],
  })) as readonly [`0x${string}`, `0x${string}`, bigint, bigint, bigint, number, boolean];

  return {
    domain: { name: "LinvestherZK-IdentityRegistry", version: "1", chainId: await deps.publicClient.getChainId(), verifyingContract: deps.identityRegistry },
    types: {
      ConfirmOwnerRotation: [
        { name: "identityId", type: "bytes32" },
        { name: "newOwner", type: "address" },
        { name: "ownerEpoch", type: "uint64" },
        { name: "nonce", type: "uint64" },
        { name: "deadline", type: "uint256" },
      ],
    },
    primaryType: "ConfirmOwnerRotation",
    message: { identityId, newOwner, ownerEpoch, nonce, deadline: nowDeadline() },
  };
}

/** Relays the user's already-signed `confirmOwnerRotation` — the
 * relayer never constructs or alters this signature, only transmits it
 * (per relayer.ts's own invariant). `challenge` must be exactly what
 * `buildConfirmOwnerRotationChallenge` produced and the user signed —
 * the relayer does not re-derive or second-guess these values. */
export async function confirmIdentityCreation(
  deps: RelayFlowDeps,
  challenge: ConfirmOwnerRotationChallenge,
  signature: `0x${string}`,
): Promise<{ txHash: `0x${string}` }> {
  const { identityId, newOwner, ownerEpoch, nonce, deadline } = challenge.message;
  const txHash = await deps.relayer.relay({
    dedupKey: `confirm:${identityId}`,
    to: deps.identityRegistry,
    data: encodeFunctionData({
      abi: identityRegistryAbi,
      functionName: "confirmOwnerRotation",
      args: [identityId, newOwner, ownerEpoch, nonce, deadline, signature],
    }),
  });
  await waitForSuccessfulReceipt(deps.publicClient, txHash);
  return { txHash };
}
