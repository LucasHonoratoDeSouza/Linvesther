// An identity's optional public name and bio (ProfileRegistry): same
// challenge-then-relay shape as the other owner-signed commands — the
// caller signs the exact EIP-712 payload, the relayer only submits it.
import { encodeFunctionData, type PublicClient } from "viem";
import { profileRegistryAbi } from "./abi.js";
import { nowDeadline, waitForSuccessfulReceipt, type RelayFlowDeps } from "./relayFlow.js";

export interface SetProfileChallenge {
  domain: { name: string; version: string; chainId: number; verifyingContract: `0x${string}` };
  types: { SetProfile: { name: string; type: string }[] };
  primaryType: "SetProfile";
  message: { identityId: `0x${string}`; name: string; bio: string; nonce: bigint; deadline: bigint };
}

export interface OnChainProfile {
  name: string;
  bio: string;
  /** Epoch seconds of the last change; 0 when nothing was ever set. */
  updatedAt: number;
}

export async function readProfile(publicClient: PublicClient, profileRegistry: `0x${string}`, identityId: `0x${string}`): Promise<OnChainProfile> {
  const [name, bio, updatedAt] = (await publicClient.readContract({
    address: profileRegistry,
    abi: profileRegistryAbi,
    functionName: "profileOf",
    args: [identityId],
  })) as readonly [string, string, bigint];
  return { name, bio, updatedAt: Number(updatedAt) };
}

export async function buildSetProfileChallenge(
  deps: RelayFlowDeps,
  profileRegistry: `0x${string}`,
  identityId: `0x${string}`,
  name: string,
  bio: string,
): Promise<SetProfileChallenge> {
  const nonce = (await deps.publicClient.readContract({
    address: profileRegistry,
    abi: profileRegistryAbi,
    functionName: "nonces",
    args: [identityId],
  })) as bigint;
  return {
    domain: { name: "LinvestherZK-ProfileRegistry", version: "1", chainId: await deps.publicClient.getChainId(), verifyingContract: profileRegistry },
    types: {
      SetProfile: [
        { name: "identityId", type: "bytes32" },
        { name: "name", type: "string" },
        { name: "bio", type: "string" },
        { name: "nonce", type: "uint64" },
        { name: "deadline", type: "uint256" },
      ],
    },
    primaryType: "SetProfile",
    message: { identityId, name, bio, nonce, deadline: nowDeadline() },
  };
}

export async function setProfile(deps: RelayFlowDeps, challenge: SetProfileChallenge, signature: `0x${string}`): Promise<{ txHash: `0x${string}` }> {
  const { identityId, name, bio, nonce, deadline } = challenge.message;
  const txHash = await deps.relayer.relay({
    dedupKey: `setProfile:${identityId}:${nonce}:${signature}`,
    to: challenge.domain.verifyingContract,
    data: encodeFunctionData({ abi: profileRegistryAbi, functionName: "setProfile", args: [identityId, name, bio, nonce, deadline, signature] }),
  });
  await waitForSuccessfulReceipt(deps.publicClient, txHash);
  return { txHash };
}
