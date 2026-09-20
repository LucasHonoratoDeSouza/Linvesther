# RISC Zero feasibility benchmark

Self-contained RISC Zero project that measures guest execution and proving
cost at the 10k-event scale required by the spec, and
demonstrates that a receipt which did not go through real proving is
rejected by verification.

## Scope

The guest (`methods/guest`) folds a batch of synthetic, ledger-shaped events
(`{id: u64, delta: i64}`) into a running balance and a tamper-evident
fingerprint, committing `(count, balance, fingerprint)` to the journal. This
is **not** the reconciliation guest of the reference protocol — it is a
bounded, representative workload sized to measure feasibility, not to prove
any real financial claim. Event data is generated deterministically by a
seeded xorshift64 PRNG; no real account or trade data is used or required.

The host (`host/src/main.rs`):

1. Runs one execution-only pass (no proving) to record guest cycle count.
2. Runs `BENCH_REPS` full proving repetitions, timing each with a wall clock,
   to estimate p50/p95 proving latency.
3. Verifies the real receipt from the last repetition against `METHODS_ID`.
4. Constructs a `FakeReceipt` carrying the same (genuine) journal bytes but
   no real proof, and asserts that `Receipt::verify_with_context` with a
   default (non-dev-mode) `VerifierContext` rejects it — this is the "modo
   fake proibido" / "fake receipt rejeitada" requirement.
5. Reports receipt size and peak resident memory (`VmHWM` from
   `/proc/self/status`).

`host/tests/guest_fold_test.rs` checks the guest's fold logic itself,
execution-only (no proving, seconds not minutes): the committed journal for
a small hand-computed vector matches an independently computed expected
balance and fingerprint, an overflowing balance halts the guest instead of
wrapping, an empty batch commits a zeroed journal, and the compiled image ID
is non-trivial. This is the fast regression check for the fold; the
expensive proving path (real receipt verification, fake receipt rejection)
is only exercised by the full `make check` run below.

## Running

```sh
make check                                   # fast tests, then 10,000 events / 3 repetitions
make check BENCH_EVENTS=200 BENCH_REPS=1     # fast tests, then a quick smoke proving run
cargo test --release -p host --test guest_fold_test   # fast tests only, no proving
```

Proving is CPU-bound and has a fixed per-call floor (zkVM setup plus at
least one padded segment) independent of workload size, so small smoke runs
are not representative of the 10k-event timing — they only validate the
pipeline.

## Measured result

See `report.md` for the actual run recorded for this delivery: hardware,
p50/p95, cycle counts, receipt size and memory. The sample size for the
percentile estimate is small (`BENCH_REPS` repetitions of one multi-minute
proving run each), which `report.md` states explicitly rather than implying
statistical precision the sample doesn't have.

## Hardware and cost

This experiment measures wall-clock time on whatever machine runs it; it
does not include GPU acceleration (no CUDA/Metal backend was exercised) or
Bonsai remote proving. `report.md` records the actual hardware used. Cost
projections beyond that hardware (e.g., cloud GPU pricing) are out of scope
for this delivery and are not claimed.
