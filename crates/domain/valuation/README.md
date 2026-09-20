# Valuation at a common cut

`NAV(t) = Σ quantity(asset,t) × price(asset,t)` in the spot profile, per
the protocol specification.

- `reconstruct.rs`: adjusts a non-atomic per-asset balance snapshot
  forward or backward to a common cut time using surrounding ledger
  deltas. An event whose timestamp exactly coincides with the snapshot
  instant is ambiguous (cannot tell if the snapshot already reflects it)
  and refuses the reconstruction — never resolved by a tie-break.
- `valuation.rs`: sums `quantity × price` across every asset. Free and
  locked balances are combined exactly once (`BalanceSnapshot::total`).
  A single asset with an ambiguous reconstruction or no price entry
  refuses the *entire* valuation, not a partial NAV silently missing that
  asset's contribution.

Price data is supplied by the caller (`prices: &BTreeMap<String, Decimal>`)
— this crate does not depend on any adapter (e.g.
`services/collector/binance/prices`); the core does not import a venue
SDK, per the architecture rules.
