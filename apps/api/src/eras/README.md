# Strategy eras

Lets an owner declare that their track entered a new "era" (e.g. a
strategy change) at a point in time, per the protocol specification's
`StrategyEra`: a timestamped label and a digest of the free-text
description the owner attaches to it.

## What this is not

A `StrategyEra` is a metadata marker, **not** a proof that the
underlying algorithm actually changed. Nothing here re-verifies the
strategy; it only records that the owner claims a transition happened,
at a given time, with a given description (hashed so its exact wording
can be checked later without republishing it every time).

## The one invariant this module enforces

Per proofs.md: "Nunca rotular recorte favorável como lifetime" (never
label a favorable cutout as the lifetime). Concretely:

- `lifetimeStartMs` is fixed the first time a track's timeline is
  touched and is never moved or hidden by any later era.
- `GET /tracks/:trackId/eras` always returns the **full** timeline —
  every declared era, never a subset.
- `GET /tracks/:trackId/eras/:eraId` returns a single era, but the
  response is an `EraScopedView` that is always `isExcerpt: true` and
  always carries the real `lifetimeStartMs` alongside it, so a caller
  can never present an era-scoped excerpt as if it were the whole
  history.

`MemoryEraStore` has no operation that removes an era or edits
`lifetimeStartMs` — appending is the only mutation available.

## Routes

- `POST /tracks/:trackId/eras` — requires a session; declares a new
  era authored by the signed-in address.
- `GET /tracks/:trackId/eras` — public; the full timeline.
- `GET /tracks/:trackId/eras/:eraId` — public; one era, always paired
  with `isExcerpt: true` and the real `lifetimeStartMs`.

## Tests

- `tests/e2e/eras/eras.e2e.test.ts` (gate: `check-e2e`) drives every
  route through the real Fastify app via `.inject()`: a new era
  preserves the track's `lifetimeStartMs` and declared label; a second
  era never moves an already-recorded `lifetimeStartMs`; declaring
  without a session is rejected; reading the timeline needs no
  session; an excerpt view always carries `isExcerpt: true` and the
  real `lifetimeStartMs`; an unknown era id is rejected rather than
  silently falling back to the full timeline.
