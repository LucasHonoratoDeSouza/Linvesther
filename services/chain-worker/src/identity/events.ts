// Decodes IdentityRegistry/AccountRegistry event logs into typed
// domain events, using the real compiled ABI (abi.ts) — never a
// hand-typed event signature, so a contract change that isn't
// re-exported here fails to decode loudly instead of silently
// decoding as the wrong shape.
import { decodeEventLog, type Log } from "viem";
import { accountRegistryAbi, identityRegistryAbi } from "./abi.js";

export type IdentityDomainEvent =
  | { name: "IdentityCreated"; identityId: `0x${string}`; owner: `0x${string}`; identityType: number; createdAt: bigint }
  | { name: "TrackCreated"; identityId: `0x${string}`; trackId: `0x${string}`; financialProfileId: `0x${string}`; denominationCommitment: `0x${string}` }
  | { name: "OwnerRotationProposed"; identityId: `0x${string}`; currentOwner: `0x${string}`; pendingOwner: `0x${string}`; ownerEpoch: bigint }
  | { name: "OwnerRotated"; identityId: `0x${string}`; previousOwner: `0x${string}`; newOwner: `0x${string}`; newOwnerEpoch: bigint };

export type AccountDomainEvent =
  | { name: "AccountRegistered"; trackId: `0x${string}`; accountId: `0x${string}`; venueId: `0x${string}`; environment: number; registeredAt: bigint }
  | { name: "AccountActivated"; trackId: `0x${string}`; accountId: `0x${string}`; eligibleFrom: bigint; reasonHash: `0x${string}` }
  | { name: "AccountRemoved"; trackId: `0x${string}`; accountId: `0x${string}`; removedAt: bigint; reasonHash: `0x${string}` };

export type DomainEvent = IdentityDomainEvent | AccountDomainEvent;

const IDENTITY_EVENT_NAMES = ["IdentityCreated", "TrackCreated", "OwnerRotationProposed", "OwnerRotated"] as const;
const ACCOUNT_EVENT_NAMES = ["AccountRegistered", "AccountActivated", "AccountRemoved"] as const;

/** Returns `null` for a log this module has no domain meaning for
 * (e.g. `EIP712DomainChanged`, or a log from an unrelated contract) —
 * never guesses, never decodes a log as the wrong event shape. */
export function decodeDomainEvent(log: Log): DomainEvent | null {
  try {
    const identityDecoded = decodeEventLog({ abi: identityRegistryAbi, data: log.data, topics: log.topics });
    if ((IDENTITY_EVENT_NAMES as readonly string[]).includes(identityDecoded.eventName)) {
      return { name: identityDecoded.eventName, ...identityDecoded.args } as IdentityDomainEvent;
    }
  } catch {
    // Not an IdentityRegistry event — fall through to AccountRegistry.
  }
  try {
    const accountDecoded = decodeEventLog({ abi: accountRegistryAbi, data: log.data, topics: log.topics });
    if ((ACCOUNT_EVENT_NAMES as readonly string[]).includes(accountDecoded.eventName)) {
      return { name: accountDecoded.eventName, ...accountDecoded.args } as AccountDomainEvent;
    }
  } catch {
    // Not an AccountRegistry event either.
  }
  return null;
}
