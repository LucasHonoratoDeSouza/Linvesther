// Identity/account lifecycle, per the protocol specification's state
// diagram (`PendingRegistration -> PendingBaseline -> ...`) and
// the acceptance criteria: owner creates/binds/removes/rotates through asynchronous
// states; a third party never modifies another owner's identity.

export type IdentityState = "pending_registration" | "active" | "pending_rotation";

export interface Identity {
  id: string;
  owner: `0x${string}`;
  createdAt: string;
  state: IdentityState;
  /** Set only while `state === "pending_rotation"`: the owner a
   * confirmed rotation will switch to. The current owner remains
   * authoritative — and the only one who can issue further commands —
   * until confirmation actually lands. */
  pendingOwner?: `0x${string}`;
}

export type AccountState = "pending_binding" | "active" | "pending_removal" | "removed";

export interface Account {
  id: string;
  identityId: string;
  state: AccountState;
}

export class LifecycleError extends Error {
  constructor(
    public readonly code: "not_found" | "forbidden" | "invalid_state",
    message: string,
  ) {
    super(message);
  }
}
