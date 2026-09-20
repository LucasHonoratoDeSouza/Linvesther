# zkvm-methods (performance)

Host-facing package for the performance claims guest. `build.rs`
calls `risc0_build::embed_methods()`, which cross-compiles
`guest/` (a separate, isolated Cargo workspace — see `guest/Cargo.toml`
and `guest/README.md`) to `riscv32im-risc0-zkvm-elf` and generates
`GUEST_ELF`/`GUEST_ID`, re-exported from `src/lib.rs`.

## Tests

`tests/guest_test.rs`:

- `a_real_proof_verifies_and_its_journal_is_tamper_evident`: runs real
  RISC Zero proving (not dev-mode) over a well-formed, A0-signed input,
  verifies the receipt, decodes the journal into every claimed field
  (origin digest, period, TWR index, MDD, capital), then confirms a
  tampered journal fails verification and the receipt does not verify
  against an unrelated image ID.
- `execution_rejects_a_tampered_envelope_without_proving` /
  `execution_rejects_an_unreconciled_ledger_without_proving`: execution
  only (no proving spent) — confirms the guest genuinely panics, rather
  than silently proceeding, on a tampered origin or an unreconciled
  ledger.

`SourceEnvelope`/`GuestInput`/`GuestOutput` are duplicated here from
`guest/src/{envelope,lib}.rs` for the same cross-workspace reason
documented in `guest/README.md`; the two are kept in sync by hand and
interoperate only through serde's wire format, matching the pattern
already used between `experiments/risc0-benchmark`'s host and guest.

## Out of scope

Bonsai/remote proving, method-ID pinning against a trust manifest, and
recursive per-checkpoint composition are not part of this guest.
