# recovery

Infrastructure and recovery, per
the operations design's targets table and
"Segredos e retenção" section. "Done when": a restore ensaio meets RPO/
RTO; isolation and retention hold; objects are verifiable in mirror; a
91-day simulation only permits purging raw inputs behind a real
validated recursive receipt that continues the track, or else retains
inputs integrally.

## Scope

- `restore_drill.rs` — `verify_restore_drill` derives RPO (time between
  the last backup and a failure) and RTO (time between the failure and
  restore completing) from a drill's own recorded timestamps and checks
  them against `RestoreTargets::DEFAULT` (RPO ≤ 15 min, RTO ≤ 4 h) — it
  never assumes the targets were met.
- `retention.rs` — `DataCategory::{Credential, PrivateActive, Backup}`
  each carry their own deadline (24 h / 30 days / 35 days) from a
  request timestamp; `check_purged_by_deadline` is isolated per category
  — a credential overdue does not imply a backup is overdue.
- `mirror.rs` — `verify_object_mirrored` hashes a primary object and its
  mirror copy and compares them; a partially-synced or corrupted mirror
  is caught by content, never assumed correct because a copy exists.
- `purge_gate.rs` — `evaluate_purge_gate` is the 91-day simulation gate:
  before `SIMULATION_DAYS` (91) it always retains; at or past that point
  it only returns `Purge` given
  `RecursiveReceiptStatus::Validated` — there is no other path to
  `Purge` in this function. This codebase has no recursive proof
  composition implemented yet (see `zkvm/methods/performance`'s scope
  notes), so every real call site today passes `NotAvailable` and the
  gate always retains integrally, exactly as the operations design requires
  when recursion isn't implemented or validated.

`../docker-compose.yml` is the local dev infrastructure layout named in
the operations design (PostgreSQL, S3-compatible storage via MinIO, Anvil) —
authored as a real deliverable, but not brought up by this crate's own
tests (see "Out of scope").

## Tests

Four files, one per module above: restore drills within/over each
target individually and both at once; each retention category's
deadline and cross-category isolation; an identical vs. diverged vs.
truncated mirror; the purge gate before/at/after the simulation window
crossed with both receipt statuses, plus an explicit test documenting
that every real call in this repo today retains (no recursion exists
yet to validate a receipt).

## Out of scope

This crate does not bring up `docker-compose.yml`'s containers or run a
real backup/restore against them — doing so would consume this
development machine's actual resources rather than exercising a
hermetic test, and this repository already hit real memory limits
running RISC Zero proving locally (see `crates/verifier`'s README). The
restore/retention/mirror/purge-gate *decision logic* is what's tested
here, independent of which storage engine or container runtime actually
executes it in a deployed environment.
