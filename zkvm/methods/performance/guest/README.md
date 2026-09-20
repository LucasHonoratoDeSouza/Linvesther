# guest

Performance claims guest, per the protocol specification: "Guest
inicial prova reconciliação spot, TWR amostrado, MDD diário, duração
elegível e predicates de capital/retorno/MDD."

## Scope

This package is both a library (`src/lib.rs` — pure, no `risc0_zkvm`
guest-only APIs) and the zkVM entry point (`src/main.rs`, compiled to
`riscv32im-risc0-zkvm-elf` via `risc0_build::embed_methods()` from
`../build.rs`). The library is what actually runs inside the zkVM and
what `tests/` and `../tests/` exercise directly, so nothing here can
drift between "what is proven" and "what is tested."

- `envelope::SourceEnvelope` and its `digest()` mirror
  `services/collector/attestation::envelope::SourceEnvelope` (same
  the protocol specification "SourceEnvelope v1" canonical encoding;
  duplicated, not shared, because `services/` and `zkvm/` are separate
  Cargo workspaces).
- `origin::verify_origin` recomputes that digest from the envelope's own
  witnessed fields — never trusts a precomputed digest — and checks an
  A0 ECDSA signature against it.
- `compute::{reconcile, twr_index, max_drawdown_bp}` reconcile the ledger
  against the NAV series and compute a chained TWR index and MDD, in
  integer fixed-point (scale `1_000_000`, never `f64`).
- `run()` ties these together and is what `main()` calls before
  committing the journal; any failure means no journal is committed.

## Out of scope

This is the *initial* performance claims guest. It does not yet compute Sharpe/
Sortino/CAGR (a later guest per proofs.md), does not do recursive
per-checkpoint composition, and uses one NAV sample per day with no
external-flow modeling — a real TWR crate (`crates/domain/returns`)
already exists with the full event-stream model; this guest is a
minimal, self-contained, integer-only reimplementation scoped to what
the acceptance criteria asks for, not a port of that crate.
