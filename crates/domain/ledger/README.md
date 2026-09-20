# Canonical ledger

Reconciles per-asset balances from adapter-normalized `LedgerEvent`s per
the protocol specification. Adapter-agnostic by design: `source_namespace`
is an opaque string the adapter defines, not an enum this crate hardcodes
— the core does not branch on vendor names.

- A retry re-ingesting the identical event (same `(namespace, source_id)`,
  same content) is a no-op.
- An event with the same key but different content is rejected
  (`LedgerError::Conflict`); the original record and the balance it
  already produced are both preserved untouched.
- A fee is a separate field from `legs`, applied to the balance exactly
  once, never folded into or skipped from a principal leg.
- Balance arithmetic uses `rust_decimal` (fixed-precision, no `f64`), per
  the protocol's precision rules.
- A malformed leg amount rejects the whole event before any balance is
  mutated — never a partial application.

Consumes the output of `services/collector/binance/trades` and
`services/collector/binance/flows` (via a conversion the collector
performs, not this crate) — see those crates' own scope notes.
