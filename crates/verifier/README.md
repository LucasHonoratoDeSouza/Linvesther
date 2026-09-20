# verifier

Public verifier and SDK, per the protocol specification's
"Algoritmo de verificação pública" and the acceptance criteria: the CLI
verifies without an official API; tamper/root/image/chain/coverage all
fail correctly; offline verification only asserts `asOf`, never current
validity.

## Scope

Five independent checks, each producing its own `CheckOutcome`
(`Pass`/`Fail(reason)`/`Unavailable(reason)`) rather than a single
opaque boolean (per proofs.md step 10):

- `bundle::Bundle::verify_hashes` — **tamper**: recomputes every
  manifested file's SHA-256 from its actual content. Reimplements the
  same scheme as `apps/api/src/exports/publicBundle.ts` in Rust
  (different language/workspace, same hash), so a bundle produced there
  verifies here as-is.
- `root::verify_account_set_root` — **root**: recomputes a Merkle root
  over the claimed membership (reusing `commitments::MerkleTree`)
  and compares it to the bundle's claimed `accountSetRoot`. Any account
  added, removed or reordered changes the root.
- `receipt::verify_receipt` — **image**: decodes a real RISC Zero
  receipt and verifies it against a trusted image ID — compiled into
  this crate from `zkvm-methods`, never accepted as a
  bundle-supplied override — then checks the journal byte-for-byte.
  Rejects a receipt that verifies for the wrong image ID or whose
  journal was swapped after proving.
- `anchor::verify_anchor` — **chain**: checks a claimed anchor
  (tx hash + block number) against a set of trusted anchors the
  verifier operator supplies independently (never read from the bundle
  — proofs.md step 2 explicitly warns against trusting the interested
  party's own submission for this).
- `coverage::verify_coverage` — **coverage**: recomputes gaps with
  `checkpoint::derive_gaps` from calendar policy alone and
  compares them to the bundle's claimed gaps — a bundle that omits a
  real gap, or invents one the calendar doesn't support, fails.

`verify::verify` orchestrates all five into one `VerificationReport`.
`report::Mode::Offline { as_of_ms }` can only ever render as
`VALID_AS_OF(<timestamp>)` on success — there is no `Mode` variant and
no code path that could print "VALID" or "CURRENT" for an offline
result.

`src/bin/cli.rs` (`linvesther-verify`) runs this whole pipeline from a
local bundle directory plus a `--trusted-anchors` file and calendar
policy flags — no network call anywhere, matching "CLI verifica sem API
oficial."

## Tests

- `tests/bundle_test.rs`, `tests/root_test.rs`, `tests/anchor_test.rs`,
  `tests/coverage_test.rs`, `tests/report_test.rs` — fast, no proving:
  each of the five failure modes (except image) triggered directly, plus
  the offline/online summary wording.
- `tests/receipt_test.rs` — the real "image" check, against a genuine
  RISC Zero receipt (not dev-mode). **Marked `#[ignore]`**: real proving
  needs more free RAM than a typical desktop session reliably has
  alongside it (~15 min; observed to push this development machine to
  under 1 GB available and freeze once). Run it explicitly with
  `cargo test -p verifier --test receipt_test -- --ignored --test-threads=1`
  when the machine has headroom, or in CI/a machine with more memory to
  spare. `zkvm/methods/performance/tests/guest_test.rs` already proves
  this exact guest for real elsewhere in the repo; this test additionally
  proves that `verify_receipt` correctly wraps that verification and its
  journal-equality check.

## Out of scope

No online mode (polling a live indexer/trust manifest to upgrade a
report beyond `VALID_AS_OF`), no bundle-format auto-detection beyond the
fixed file names this crate expects, no packaged SDK library beyond the
crate's own public API (a thin wrapper crate for other languages, if
ever needed, is future work).
