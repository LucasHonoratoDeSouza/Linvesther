# checkpoint

Checkpoint composition and gaps, per the protocol specification's
"Continuidade" section and its `Checkpoint` definition.

## Scope

- `calendar::derive_gaps` derives every missed interval strictly from a
  `CalendarPolicy` (cadence + grace period), the last committed
  checkpoint's end, and the current time. It takes no user-supplied or
  "voluntary" event — any verifier holding only the calendar and the last
  checkpoint computes the same gaps.
- `continuity::segments_between` splits a track's span into continuous
  segments broken at each gap. `continuity::recover_late` records data
  that arrives after a gap's deadline as a `LateSupplement`; it never
  mutates or removes the gap it recovers, so segments computed before and
  after a late recovery are identical — continuity is never restored
  retroactively.
- `chain::link_next` confirms a checkpoint links to its predecessor: the
  sequence must increment by exactly one, and `previous_digest` must
  equal the predecessor's digest. A checkpoint that fails this is not
  accepted as the track's next state.

## Out of scope

This crate models the calendar/continuity/chaining primitives only. It
does not implement checkpoint state transitions
(`collecting → reconciled → committed → anchored → finalized`) beyond the
`CheckpointState` enum, nor the coverage/calculation/correction
orthogonal states, nor anchoring or proof attachment.
