# binance-connect

The real "connect your Binance account" product flow — the piece that
makes `services/collector/binance/live-client`'s proven connectivity
actually work for any person, not just a manually-run test script.

## Why a subprocess, not a TypeScript reimplementation

Every other cross-Rust-boundary module in `apps/api` (`multi-account`,
for example) reimplements the relevant domain rules independently in
TypeScript, because `apps/api` cannot import a Rust crate at runtime.
Binance connectivity is different: the pagination/normalization/
signing logic in `binance-trades`/`binance-flows`/`binance-catalog`/
`collector-credentials` is substantial, already real and tested, and
duplicating it in TypeScript would risk drift from the Rust side that
actually talks to Binance. Instead, this module spawns
`services/collector/binance/worker`'s compiled binary as a subprocess
per request, via `worker.ts`'s `invokeWorker`.

Credentials go to the subprocess via **stdin only** — never a CLI
argument (visible in the process list to anyone on the same machine)
— and this module never logs or returns the raw key/secret in any
response.

## Ownership: `isOwner` and the self-ownership rule

Every route below is owner-only, checked via `routes.ts`'s `isOwner`
helper: an explicit `accountOwners` entry (the same map
the account routes use) always takes precedence when
present, but if `accountId` has no entry at all, an address is treated
as owning the accountId that equals its own address. This lets a
person connect and read their own Binance data (e.g. through
`apps/web/app/portfolio`) as soon as they sign in, without first going
through a separate `bindAccount` lifecycle registration — it never
grants access to any *other* accountId, only ever the caller's own.

## Routes

- `POST /accounts/:accountId/binance-connection` — owner-only (same
  `accountOwners` map `/accounts/:accountId/secret` already uses).
  Body: `{ apiKey, apiSecret, symbols? }`. Spawns `binance-worker
  connect`, which validates the key is read-only, stores it encrypted,
  runs one sync cycle immediately, and returns a summary. A rejected
  key (e.g. one with trade/withdraw permissions) comes back as a 422
  with the worker's own error message — which never contains the raw
  key/secret. `symbols` is optional and only controls a one-time
  *historical* trade backfill (`myTrades` needs a symbol per call);
  every trade from connection time forward is captured in real time
  via the account's WebSocket user data stream regardless of symbol —
  see `services/collector/binance/worker`'s README.
- `GET /accounts/:accountId/binance-connection` — owner-only. Spawns
  `binance-worker status` and returns the connection's current state
  (connected, last synced, trade/flow counts).
- `GET /accounts/:accountId/binance-nav` — owner-only. Spawns
  `binance-worker nav` and returns a real, current NAV computed from
  the account's real balance and real market prices. A single
  point-in-time snapshot, not a historical series.
- `GET /accounts/:accountId/binance-performance` — owner-only. Spawns
  `binance-worker performance` and returns a real historical
  performance summary: daily NAV/returns (chart data), plus Sharpe/
  Sortino/MDD/CAGR/win-rate. Each metric is independently `null`,
  paired with a `*Unavailable` reason, when its own minimum sample
  isn't met — never a fabricated number. Can take on the order of a
  minute for an account with months of real history — see
  `services/collector/binance/worker`'s README for the documented
  performance gap behind that.
- `GET /accounts/:accountId/binance-performance-proof` — owner-only.
  Spawns `binance-worker prove-performance` and returns a portable,
  independently-verifiable ZK proof (real RISC Zero proving, not
  dev-mode) of return/max-drawdown over the account's real history —
  never the raw trades, flows, balance or account identifier. **This
  can take a very long time** (multiple hours were observed against a
  real ~110-day account history during this delivery, even after
  fixing a first attempt that OOM-killed the whole host) — see
  `services/collector/binance/worker`'s README, "Real ZK performance
  proof", for why and for the memory-containment convention
  (`systemd-run`+cgroup) used when running it manually. No UI is wired
  to this route yet for exactly that reason.

## What actually happens when someone connects

1. This route validates the caller owns `accountId` (session + the
   same ownership check every other owner-gated route in this API
   uses).
2. The Rust worker validates `apiRestrictions` for real, against the
   real Binance API — a key with any write/trade/transfer capability
   is refused before anything else happens.
3. The key is encrypted (AES-256-GCM) and stored in Postgres.
4. One sync cycle runs immediately: real trades/deposits/withdrawals/
   catalog are fetched and persisted.
5. From then on, `services/collector/binance/worker`'s `scheduler`
   process re-syncs this connection on its own, with zero further
   action from this API.

This was run for real during this delivery — a genuine read-only key,
through this exact HTTP route, against a real Postgres and a real
Binance account. See
`tests/e2e/binance-connect/binanceConnect.e2e.test.ts` for that test
(skipped by default; it needs real credentials in the environment) and
`services/collector/binance/worker/README.md` for what the worker
itself does and does not do.

## Tests

- `apps/api/test/binanceConnect.test.ts` — `invokeWorker` against a
  small fake "worker binary" script (not the real Rust binary), so
  this runs fast in the default gate: parses stdout JSON, surfaces the
  worker's own error message on failure, distinguishes a malformed-
  JSON failure, and handles a binary that does not exist.
- `tests/e2e/binance-connect/binanceConnect.e2e.test.ts` — the real
  path, through the real Fastify app, the real Rust binary, a real
  local Postgres and a real Binance account. Skipped unless
  `BINANCE_API_KEY`/`BINANCE_API_SECRET`/`DATABASE_URL`/
  `BINANCE_WORKER_ENCRYPTION_KEY` are all present — see that file's
  doc comment for how to run it for real.
- `tests/e2e/web/portfolio.spec.ts` — a real browser (Playwright)
  driving `apps/web/app/portfolio` against this API for real. Runs
  with a fake Binance key on purpose (no credentials needed in CI): it
  proves a bad key surfaces as a real, visible failure through the
  whole stack rather than a fabricated success. The credentialed path
  (real NAV/performance rendering) is exercised by
  `binanceConnect.e2e.test.ts` above.
