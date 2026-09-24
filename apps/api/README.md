# api

Fastify API for Linvesther: sign-in, the identity lifecycle, claims, public
profiles and the account-connection routes.

## Modules

| Module | Responsibility |
| --- | --- |
| `auth/` | Passkey (WebAuthn) and password-vault sign-in, sessions, and the rate limiter. |
| `lifecycle/` | Identity and account commands (`POST /identities`, bind, remove, rotate). Owner-only; a state change takes effect once the chain confirms it. |
| `claims/` | Publishing and reading signed claims, and checking a claim against the owner's figures. |
| `profile/` | Combined public profile, privacy mode and the optional on-chain name and bio. |
| `binance-connect/` | Connecting exchange and broker accounts and reading their performance. |
| `readonly/` | The read-only account API under `/mcp`, for an outside agent. Six GETs, a bearer token, no writes — see [its README](src/readonly/README.md). |
| `api-tokens/` | Creating, listing and revoking the read-only tokens that reach `/mcp`. Session-only. |
| `public/`, `exports/`, `eras/`, `multi-account/` | Track projection, portable bundles, strategy eras and consolidation. They keep their data in memory. |
| `GET /public/collector` | The fingerprint of the key that signs this instance's figures, also carried as `origin` on profiles and claims. |

## Sign-in and sessions

- `auth/webauthn.ts` runs the passkey registration and authentication
  ceremonies and verifies a P-256 assertion against the stored public key.
- `auth/vault.ts` verifies the password-derived key: a nonce-consuming
  challenge for sign-in, and a plain signature check over an already-hashed
  payload where the challenge is itself a digest, such as consent.
- `auth/identitySignature.ts` dispatches a check to one or the other by the
  method the identity registered with. Sign-in and claim consent both go through it.
- A successful sign-in sets an `HttpOnly`, `SameSite=Lax` session cookie
  (`Secure` over HTTPS) for 30 days. With `DATABASE_URL` set, sessions, identities,
  passkeys and published claims are kept in Postgres and survive a restart;
  without it they are in memory. Sign-in challenges, traffic limits and the
  gas budget are shared in Postgres the same way, so several instances behave as
  one and a restart forgets nothing; without a database they are per process, so
  run a single instance.

## Read-only API tokens

An identity can hand out a token that reads its own account and can do
nothing else. It is a separate credential from the session cookie, kept
only as a SHA-256 hash, revocable at any moment, and accepted only by the
`/mcp` routes — `auth/requireSession.ts` reads the cookie and never a
header, `auth/requireReadOnlyToken.ts` reads the header and never a
cookie, so neither credential reaches the other's routes. See
[`src/readonly/README.md`](src/readonly/README.md).

## Limits

- A state-changing request from an origin the API does not serve is refused.
- Starting a real proof is rate-limited per session.
- Fastify's `bodyLimit` rejects an oversized payload before any route runs.
- The read-only API is capped per token, tighter than a browser's reads.

## Claims

A claim set carries the identity, track, checkpoint, period, statements,
audience, nonce and expiry. `claims/claimSet.ts` hashes the whole set as
canonical JSON (RFC 8785) with SHA-256, so every field is covered by the owner's
signature.

- `POST /claims/disclose` checks that the statements are true of the owner's
  current figures, then verifies consent, then stores the claim.
- `GET /claims/mine` lists the owner's claims.
- `GET /claims/:digest` re-checks expiry and audience.
- `GET /public/claims/:digest` is the public read.

## Public reads

- `GET /public/profiles/:address` and `.../series` return the combined
  record as percentages only.
- Nothing returned here contains a balance, position, trade, credential or
  exchange identifier.

## Development

```sh
pnpm --filter @linvestherzk/api test
pnpm --filter @linvestherzk/api typecheck
pnpm --filter @linvestherzk/api start
```

End-to-end suites that go through the real Fastify app live in
[`tests/e2e`](../../tests/e2e).
