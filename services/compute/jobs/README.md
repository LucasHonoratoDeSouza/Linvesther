# compute-jobs

Transactional job queue and outbox, per
the operations design's "Jobs, retries e
recuperação" section.

## Scope

- `job::Job` mirrors the fields the operations design names: `id`, `job_type`,
  `tenant`, `dedup_key`, `payload_digest`, `available_at_ms`,
  `lease_until_ms`, `attempt`, `state`, `last_error_code`.
- `outbox::Outbox` deduplicates effects by idempotency key. Replaying the
  same sequence of effects after a simulated crash — even from the very
  start — never re-records an effect that already succeeded, so a crash
  between any two effects is resumed without duplication.
- `lease::try_acquire` blocks reacquisition only while a lease's
  `lease_until_ms` is still in the future; once it has passed, any worker
  can acquire the job, regardless of who held the lease before.
- `retry::decide` returns `RetryAfter` with an exponential, capped delay
  while under the attempt budget, and `DeadLetter` — carrying the failure
  cause and the job's original `available_at_ms` deadline — once the
  budget is exhausted or the error is permanent (permission/schema/UID
  mismatches skip straight to dead-letter, never looping blindly).
  `jittered_delay_ms` takes the random unit as a parameter so its
  `[0, delay_ms]` bound is verified for every possible draw, not just
  whatever a hidden RNG produces in one test run.

## Out of scope

This crate models the queueing/retry/outbox invariants only. It does not
implement PostgreSQL `FOR UPDATE SKIP LOCKED` polling, the per-track
checkpoint composition lock, or the remote-effect reconciliation step
(querying hash/nonce/tx before retrying) — those require a real database
and remote adapters that this task does not deliver.
