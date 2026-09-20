import type { InternalTransfer, MemberAccount, TrackMembership } from "./types.js";
import { validateInternalTransfer } from "./aggregate.js";

export class MemoryMultiAccountStore {
  private readonly memberships = new Map<string, MemberAccount[]>();
  private readonly transfers = new Map<string, InternalTransfer[]>();

  addMember(trackId: string, member: MemberAccount): TrackMembership {
    const existing = this.memberships.get(trackId) ?? [];
    const updated = [...existing, member];
    this.memberships.set(trackId, updated);
    return { trackId, members: updated };
  }

  setGap(trackId: string, accountId: string, hasGap: boolean): void {
    const members = this.memberships.get(trackId) ?? [];
    this.memberships.set(
      trackId,
      members.map((member) => (member.accountId === accountId ? { ...member, hasGap } : member)),
    );
  }

  getMembership(trackId: string): TrackMembership | undefined {
    const members = this.memberships.get(trackId);
    if (!members) return undefined;
    return { trackId, members };
  }

  /** Records a transfer, validating reconciliation first and applying
   * its fee-adjusted effect to both member's NAV atomically — a
   * rejected transfer never partially applies. */
  recordTransfer(trackId: string, transfer: InternalTransfer): TrackMembership {
    validateInternalTransfer(transfer);
    const members = this.memberships.get(trackId) ?? [];
    const updated = members.map((member) => {
      if (member.accountId === transfer.fromAccountId) {
        return { ...member, navMicros: member.navMicros - transfer.outgoingMicros };
      }
      if (member.accountId === transfer.toAccountId) {
        return { ...member, navMicros: member.navMicros + transfer.incomingMicros };
      }
      return member;
    });
    this.memberships.set(trackId, updated);
    this.transfers.set(trackId, [...(this.transfers.get(trackId) ?? []), transfer]);
    return { trackId, members: updated };
  }

  getTransfers(trackId: string): InternalTransfer[] {
    return this.transfers.get(trackId) ?? [];
  }
}
