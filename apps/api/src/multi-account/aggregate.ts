import type { AggregateResult, InternalTransfer, MemberAccount } from "./types.js";

export class TransferReconciliationError extends Error {
  constructor(
    public readonly outgoing: number,
    public readonly incomingPlusFee: number,
  ) {
    super(`transfer amounts do not reconcile: outgoing ${outgoing} != incoming+fee ${incomingPlusFee}`);
    this.name = "TransferReconciliationError";
  }
}

/** Sums every member's NAV. Refuses the *entire* aggregate (not a
 * partial sum silently omitting the gapped member) if any single
 * member has a coverage gap at this cut — "gap de membro impede
 * agregado,". */
export function consolidatedNav(members: MemberAccount[]): AggregateResult {
  const gapped = members.find((member) => member.hasGap);
  if (gapped) {
    return { status: "unavailable", reason: "member_gap", accountId: gapped.accountId };
  }
  const navMicros = members.reduce((sum, member) => sum + member.navMicros, 0);
  return { status: "available", navMicros };
}

/** Validates that a transfer's outgoing amount reconciles exactly with
 * incoming+fee — "fee em trânsito" shows up only as a natural NAV
 * reduction, never as an unreconciled discrepancy. Throws rather than
 * silently accepting a mismatched transfer. */
export function validateInternalTransfer(transfer: InternalTransfer): void {
  const incomingPlusFee = transfer.incomingMicros + transfer.feeMicros;
  if (transfer.outgoingMicros !== incomingPlusFee) {
    throw new TransferReconciliationError(transfer.outgoingMicros, incomingPlusFee);
  }
}
