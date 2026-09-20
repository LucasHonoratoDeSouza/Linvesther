# multi-account

Enables tracking a strategy across more than one exchange account,
end-to-end, per the protocol specification: "Em qualquer corte,
incluir todas as contas membros... Mudança em qualquer conta afeta o
cálculo agregado, nunca só um detalhe oculto de UI."

This is a from-scratch TypeScript reimplementation of the same rules
`crates/domain/consolidation` already implements in Rust — not a
shared import, since apps/api is its own runtime independent of the
Rust domain crates, the same boundary every other apps/api module
already crosses this way.

## The three guarantees this module enforces

- **Membership is public, but only the track's owner can change it.**
  `GET /tracks/:trackId/members` needs no session; `POST
  /tracks/:trackId/members` and `POST /tracks/:trackId/transfers`
  require the signed-in address to match the track's registered owner.
- **A transfer's fee is a real NAV reduction, never silently
  absorbed.** `validateInternalTransfer` rejects (HTTP 400) any
  transfer where `outgoingMicros != incomingMicros + feeMicros` —
  "fee em trânsito" always shows up as an actual change to both
  members' NAV, not a rounding artifact.
- **One member's coverage gap blocks the whole aggregate.**
  `consolidatedNav` refuses the entire aggregate — not a partial sum
  silently omitting the gapped member — the moment any single member
  has `hasGap: true`, per the protocol's "impedir continuidade se qualquer
  membro estiver incompleto."

## Routes

- `POST /tracks/:trackId/members` — owner-only; adds a member account.
- `GET /tracks/:trackId/members` — public; the full membership list.
- `POST /tracks/:trackId/transfers` — owner-only; records a reconciled
  internal transfer, applying its fee-adjusted effect to both
  members' NAV.
- `GET /tracks/:trackId/aggregate` — public; the consolidated NAV, or
  an explicit `{ status: "unavailable", reason: "member_gap", accountId }`
  when any member is gapped.

## Tests

- `apps/api/test/multiAccount.test.ts` (gate: `check-e2e`): the
  aggregate sums cleanly with no gaps; a single gapped member refuses
  the whole aggregate; a reconciled transfer is accepted; an
  unreconciled one throws.
- `tests/e2e/multi-account/multiAccount.e2e.test.ts` drives every
  route through the real Fastify app via `.inject()`: two accounts are
  tracked end-to-end with public membership and a summed aggregate;
  adding a member without a session is rejected; a transfer's fee
  shows up as a real NAV reduction; an unreconciled transfer is
  rejected with 400; a gap on one member blocks the aggregate entirely
  (not just for that member); a third party cannot manage someone
  else's track.
