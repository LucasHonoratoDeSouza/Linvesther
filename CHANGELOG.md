# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com), and versions follow
[Semantic Versioning](https://semver.org).

## Unreleased

### Changed

- Images now stay cached for a year instead of 60 seconds after the site's
  first request for each size, which is what made them look slow to load —
  every image was already served resized and as WebP (or AVIF), but a stale
  cache re-encoded the same file on almost every request once traffic was
  light. The IBKR broker logo now goes through the same resizing and format
  negotiation as every other image on the site; it used to bypass it.

- `thiserror` upgraded from 1 to 2 across every Rust crate (none of them are on
  the zkVM guest's dependency path, so no proof's image identifier changes).

### Fixed

- Creating an identity failed with `internal_error` right after the API had been
  restarted. The indexer resumes at the highest block it stored and read that
  block's events again, and inserting an identity, track or account twice was an
  error. Those inserts now ignore a row that already exists, and overlapping
  syncs run one after another instead of walking the same blocks together.

- "Activate my account" did nothing after the page was reloaded: the password's
  key lives only in the page's memory, and the button returned without a word
  when it was gone. It now says to sign in with the password again.

### Changed

- The site declares 48, 96 and 192 pixel icons, the sizes search engines accept
  for the icon next to a result (the existing ones were 16, 32 and 512).

- `www.` and plain-http requests are redirected (308) to the canonical https
  address, so search engines do not index the same page twice.

- The landing page walkthrough video no longer has a soundtrack, so its sound
  button is gone.

### Added

- Kraken (spot) as a fourth read-only connection: a signed client for balances,
  trades, the account ledger and prices, with deposits and withdrawals read from
  the ledger instead of inferred. A key that can place orders or withdraw is
  refused, checked by trying without changing anything since Kraken cannot
  report a key's permissions. Verified against the request-signing example in
  Kraken's guide and a stand-in server; not yet against a real account. The
  permission it asks for that is easiest to miss, "Query ledger entries", sits in
  Kraken's "Data" group, and the instructions and the refusal message now say so.
  The Kraken logo is the exchange's own mark.

- Four guides in the documentation (verifiable track records, zero-knowledge
  performance, proving performance without revealing trades, and a comparison of
  verification approaches), each with `TechArticle` structured data.
- One-sentence definition of the project, used by the site metadata, `llms.txt`,
  the README and the repository description.
- `CITATION.cff` and `.zenodo.json`, so the project can be cited and archived
  with a DOI.
- Visits that carry `?utm_source=` (for example from ChatGPT search) are counted
  and appear in the server report.

- `deploy/server/probe.py` records a health sample every minute (public addresses
  through the tunnel, services, database, tunnel request counters, machine load,
  memory and disk), and `report.py` summarises availability, outages, reboots,
  latency and traffic peaks.

- `deploy/server`: production services (API, web, tunnel, Postgres) as user
  `systemd` units and a pull-based deploy that ships a commit only after its CI
  passed, keeps the previous build, and rolls back on a failed health check.
- `GET /healthz` on the API, a liveness check with no side effects.

### Security

- Stored exchange credentials are now bound to their broker, account and field
  (authenticated data), so a ciphertext copied to another row no longer
  decrypts. The key can be rotated: the previous key stays readable, and the
  worker's `rekey` re-encrypts everything (`credential-check` reports without
  changing). Existing credentials keep working and are upgraded by `rekey`.
- Errors from Interactive Brokers, Coinbase and Binance no longer repeat the
  request URL. That URL carried the Flex Web Service token (or a request
  signature) into messages shown to people and written to logs.
- Sign-in challenges are now unguessable (they used `Math.random`), expire after
  five minutes and are capped, so asking for them cannot grow memory.
- Per-client rate limits on sign-in, connecting an account, actions that cost
  gas, and public reads; a cap on concurrent worker processes; a real proof runs
  one at a time and its quota belongs to the person, not the session.
- Connect routes validate that credentials are plain, bounded strings and no
  longer fail with a 500 on a missing or malformed body.
- Internal errors return `internal_error` instead of their message.
- The database stores only a hash of each session id.
- New passwords for a password identity must have at least 10 characters.
- The API no longer exits when the database restarts or drops a connection: it
  logs the error and the pool reconnects.
- The web app sends a per-request Content-Security-Policy: a script runs only if
  it carries that request's nonce, so injected markup cannot execute. Checked in
  a real browser across ten pages, with no violations.
- The operator's gas budget is capped across all clients together
  (`RELAY_MAX_PER_HOUR`, default 300), so many clients cannot drain the relayer.
- The worker's secrets can be given as files (`NAME_FILE`), kept out of the process
  environment, and a file other users can read is refused.
- The dependency audit runs in CI (`pnpm audit`, `cargo audit`); the two accepted
  Rust advisories are listed with reasons in `.cargo/audit.toml`.
- The compose file takes `POSTGRES_PASSWORD` and `MINIO_ROOT_PASSWORD` from the
  environment; the built-in default is for a throwaway local database only.
- Security headers on the web app, and `nosniff`, `no-store` and
  `no-referrer` on API answers.
- Updated Fastify (validation-bypass and host-spoofing fixes), `ws`, `postcss`
  and `quick-xml` (memory-exhaustion fixes). `pnpm audit` reports nothing.
- The local Postgres, MinIO and Anvil ports are bound to `127.0.0.1`: the
  database password in the repository is public, so it must not be reachable
  from the network.

### Added

- Sign-in challenges, per-client traffic limits and the gas budget are kept in
  Postgres when `DATABASE_URL` is set, so several API instances behave as one and a
  restart forgets nothing. The in-memory versions remain for tests and single
  instances.
- The relayer refuses to broadcast below a balance floor (`RELAYER_MIN_BALANCE_WEI`),
  and the API answers 503 `relayer_underfunded` instead of failing mid-transaction.

- Remove a connected account from Portfolio: the stored credential and everything
  collected for it are deleted, and it stops counting in the public profile.
- A sound button on the landing page video, which autoplays muted; people who ask for
  reduced motion get a paused video with the browser's controls.

- Proofs record which collector key signed the source data. The verifier
  classifies it against a list of trusted collectors (`trust/collectors.json`)
  as trusted, self-attested, revoked or outside its validity window, and does
  not exit cleanly on data it does not trust unless `--accept-self-attested`
  is passed.
- `GET /public/collector`, and an `origin` on public profiles and claims. Public
  pages label figures as attested by a listed collector or as self-attested.
- Documentation pages "Verify a proof" and "Run it yourself".
- Sign-ins, identities, passkeys and published claims are kept in Postgres when
  `DATABASE_URL` is set, so they survive an API restart. Previously everyone who
  registered a passkey could not sign in again after a restart, and shared claim
  links disappeared.
- Sessions now expire after 30 days.
- `infra/local/up.sh` starts a complete local environment (database, local chain,
  deployed contracts and `.env.local`) that needs no outside network.

### Changed

- The session, credential and claim stores are asynchronous, with in-memory and
  Postgres implementations that share one behaviour suite.
- The CI workflow provides Postgres and Foundry to the integration and e2e jobs,
  and no longer runs a release gate that depended on a report kept out of the
  repository.

- The performance proof's journal layout gained a signer fingerprint, which
  changes the guest's image identifier. Proofs made before this change do not
  verify against the new identifier.

Planned:

- Zero-knowledge proofs for the combined record and for each disclosed claim,
  attached to on-chain checkpoints.
- Stronger data origin: authenticated sessions with exchanges and issuer-signed
  statements where institutions provide them.
- More exchange and broker connections, and additional metrics with their own
  published methods.
- A production deployment on Base after independent review.

## 0.1.0

First public version, running on the Base Sepolia test network.

### Added

- Passkey and password identities backed by smart accounts.
- Read-only connections to Binance, Coinbase and Interactive Brokers.
- Combined public profile with return, max drawdown, Sharpe ratio and win rate,
  and a full privacy mode.
- Claims: signed, exact-content statements shared as links, checked against the
  owner's current figures before they are accepted.
- Optional on-chain profile name and bio.
- Zero-knowledge proofs of performance per exchange account (RISC Zero).
- Registry contracts for identities, accounts, profiles and checkpoints.
- Documentation and a whitepaper.
