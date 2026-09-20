import { describe, expect, it } from "vitest";
import { consolidatedNav, TransferReconciliationError, validateInternalTransfer } from "../src/multi-account/aggregate.js";
import type { MemberAccount } from "../src/multi-account/types.js";

function member(overrides: Partial<MemberAccount> = {}): MemberAccount {
  return { accountId: "acc-1", joinedAtMs: 0, navMicros: 1_000_000, hasGap: false, ...overrides };
}

describe("consolidatedNav", () => {
  it("sums every member's NAV when none has a gap", () => {
    const result = consolidatedNav([member({ accountId: "a", navMicros: 100 }), member({ accountId: "b", navMicros: 250 })]);
    expect(result).toEqual({ status: "available", navMicros: 350 });
  });

  it("refuses the entire aggregate — not a partial sum — when any single member has a gap", () => {
    const result = consolidatedNav([
      member({ accountId: "a", navMicros: 100 }),
      member({ accountId: "b", navMicros: 250, hasGap: true }),
    ]);
    expect(result).toEqual({ status: "unavailable", reason: "member_gap", accountId: "b" });
  });
});

describe("validateInternalTransfer", () => {
  it("accepts a transfer where outgoing exactly equals incoming plus fee", () => {
    expect(() =>
      validateInternalTransfer({ fromAccountId: "a", toAccountId: "b", outgoingMicros: 100, feeMicros: 1, incomingMicros: 99 }),
    ).not.toThrow();
  });

  it("rejects a transfer that does not reconcile", () => {
    expect(() =>
      validateInternalTransfer({ fromAccountId: "a", toAccountId: "b", outgoingMicros: 100, feeMicros: 1, incomingMicros: 100 }),
    ).toThrow(TransferReconciliationError);
  });
});
