# local-agent

A local-execution mode for owners who don't want to hand their
exchange API key/secret to a hosted collector: this crate's logic runs
on the owner's own machine, and only a sanitized, operator-safe result
ever needs to leave it.

## What "local-only" means here

`LocalOnlyInput` carries the exchange credentials, any raw witness
bytes, and the ledger events to process — but `execute_locally` never
returns any of that. Its output type, `OperatorSafeResult`, simply has
no field for a credential or witness: `event_count`, `balance`, and a
`fingerprint` derived from the event ids. There is no sanitizing step
that could regress by forgetting to strip a field, because there is no
field to strip — the type itself is the guarantee, the same pattern
used by the export/public-projection sanitizers elsewhere in this
repo.

`canonical_result_bytes`/`canonical_result_digest` serialize only that
safe result, so the digest an operator or verifier checks against is
provably built from data that already excludes credentials and
witness bytes.

## Determinism

`execute_locally` is a pure function of `LocalOnlyInput`: the same
ledger events always fold to the same `balance` and `fingerprint`, so
`canonical_result_digest` is reproducible without needing to trust
the agent's runtime state.

## Continuity across restarts

`LocalAgentState` records only `processed_through` — how many of the
full ordered event list have already been folded in. `events_to_process`
is the only way to advance it: given the current state and the full
event list, it returns exactly the unprocessed suffix and the state to
persist next. A restart with a stale or fresh state resumes from
`processed_through`; it can't skip events (the slice starts exactly
there) or double-process them (the returned next state always equals
the full list length after processing the returned slice).

## Tests

- `tests/execution_test.rs`: the same input produces the same digest
  twice; the result reflects the actual ledger events; the credential
  and witness values (and field names) never appear anywhere in
  `canonical_result_bytes`'s output; a different ledger produces a
  different digest.
- `tests/continuity_test.rs`: a fresh agent processes every event;
  restarting after partial progress resumes with exactly the
  unprocessed events, no skip or duplicate; re-running after full
  completion is a no-op.
