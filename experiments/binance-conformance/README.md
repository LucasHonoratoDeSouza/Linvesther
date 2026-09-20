# Binance B0 conformance experiment

Self-contained Rust crate that decides `POLICY_COMPLETE` vs `INCOMPLETE`
coverage for the `binance-global-spot-v1` profile from collected evidence,
per the adapter spec's Gate B0 checklist.

## Scope

This experiment validates the **coverage decision engine**: given evidence
about permissions, pagination exhaustion, flow-family classification,
catalog archiving and balance reconciliation, it decides whether the
evidence is sufficient to claim complete coverage, and reports every
blocking reason. It also validates HMAC-SHA256 request signing against a
known vector.

It does **not**:

- perform live requests against Binance (no HTTP client is included);
- reconcile ledgers or normalize flows (that is a later task's scope);
- parse real Binance response payloads end-to-end.

## What Gate B0 still requires

Gate B0 in the spec passes "somente se um ensaio autorizado em ambiente real
confirmar" the checklist items against a live, credentialed account. No such
run has been performed in this delivery: no Binance account or API key was
available. `cargo test` / `make check` here exercise the decision engine
against synthetic evidence for every checklist item, which is necessary but
not sufficient for the real gate. Running the real gate requires:

1. A read-only API key (`enableReading=true`, all other capabilities off)
   for an identified Spot account, provided by its titular per the spec's
   consent requirement.
2. A collector that performs the actual paginated requests and produces
   `Evidence` (this crate's input type) from real responses — not built
   here.
3. Re-running the checklist against that real evidence, and archiving the
   resulting `CoverageManifest` as B0 evidence.

Until then, Binance homologation status stays `INCOMPLETE` by construction:
this experiment cannot mark itself as a live pass.

**Update (live homologation)**: item 2's HTTP client now exists —
`services/collector/binance/live-client`, tested against a real
read-only account (see that crate's README). It confirms real
connectivity, request signing, and response parsing into
`binance-trades`/`binance-catalog`/`binance-flows`' existing types, and
found and fixed two real gaps this experiment's synthetic evidence
couldn't have caught. It does **not** yet assemble this crate's
`Evidence` type or a `CoverageManifest` from those real responses —
that translation step, and re-running `evaluate()` against real
evidence, remains open.

## Layout

- `src/model.rs` — evidence types (`Permissions`, `AccountBinding`,
  `SymbolTradeCoverage`, `FlowFamilyCoverage`, `CatalogVersion`,
  `AssetReconciliation`, `Evidence`).
- `src/pagination.rs` — pure exhaustion/boundary rules shared by every
  paginated endpoint family, including the tied-timestamp boundary case.
- `src/coverage.rs` — `evaluate(&Evidence) -> CoverageReport`, the
  completeness decision and its itemized reasons.
- `src/signing.rs` — HMAC-SHA256 request signing, tested against Binance's
  own documented example vector.
- `tests/coverage_test.rs` — one test per Gate B0 checklist item.

## Running

```sh
make check
```
