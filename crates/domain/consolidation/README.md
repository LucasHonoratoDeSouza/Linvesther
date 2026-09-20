# consolidation

Multi-account consolidation, per the protocol specification's
"Múltiplas contas e consolidação" section.

## Scope

- `aggregate::consolidated_nav` sums every member account's NAV at a cut.
  Any member with a coverage gap at that cut makes the aggregate
  unavailable — never partially computed by skipping or zeroing the
  gapped member.
- `aggregate::account_added_flow` / `account_removed_flow` translate a
  member joining or leaving into an external flow equal to its exact
  entry/exit NAV, so the membership change itself creates neither profit
  nor loss.
- `transfer::link_internal_transfer` links two legs into one
  `InternalTransfer` only when both accounts are current members, both
  legs are in the same asset, the accounts differ, and the amounts
  reconcile as `outgoing == incoming + fee`. Matching value and timing
  alone is never sufficient — callers must supply a `link_id` backed by
  source/transaction evidence.
- `InternalTransfer::consolidated_external_flow` always returns zero: an
  internal transfer is never modeled as an external flow at the
  consolidated level, so it counts once, not twice. The fee is not
  modeled as a flow either — it already reduced the incoming leg, so it
  shows up naturally as a lower aggregate NAV at the next valuation.

## Verified vectors

`tests/vectors_test.rs` feeds this crate's output into
`returns::compute_twr` and reproduces the exact named vectors from
the protocol specification:

- Internal transfer without fee: NAV stays 100, return 0.
- Internal transfer with a fee of 1: NAV drops to 99, return -1%.
- Account added at its exact entry NAV: return 0.

## Out of scope

This crate does not model an explicit `in_transit` ledger account for
transfers still awaiting proof of linkage; an unlinked pair of legs
simply never reaches `link_internal_transfer` successfully, and the
consolidation for that cut stays pending at the caller.
