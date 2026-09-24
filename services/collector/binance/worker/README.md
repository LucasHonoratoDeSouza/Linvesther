# binance-worker

The real, multi-user, always-on piece of connecting a Binance account:
stores a person's read-only API credential encrypted at rest, captures
every fill in real time regardless of symbol, periodically re-syncs
deposits/withdrawals/catalog, and computes a real current NAV — without
anyone re-running a manual test each time.

This exists because `binance-live-client`'s own homologation run
proved the *connectivity* works, but nothing made it work for *any
person*: the key lived in a `.env` file, nothing was persisted, and
nothing ran on its own. This crate is that missing product layer.

## Subcommands

```text
binance-worker connect    # reads {"accountId","apiKey","apiSecret","symbols"?} JSON from stdin,
                           # validates the key is read-only, stores it encrypted, runs one sync
                           # cycle immediately (see "symbols", below), and prints a JSON summary
binance-worker status     # reads {"accountId"} JSON from stdin, prints connection status as JSON
binance-worker nav        # reads {"accountId"} JSON from stdin, computes and prints a real,
                           # current NAV snapshot from the account's real balance and real prices
binance-worker performance # reads {"accountId"} JSON from stdin, prints real daily NAV/returns
                           # plus Sharpe/Sortino/MDD/CAGR/win-rate (see history.rs, below)
binance-worker trades     # reads {"accountId","symbol"?,"sinceMs"?,"untilMs"?,"cursor"?,"limit"?}
                           # JSON from stdin, prints one page of the executions already collected
                           # (see trade_log.rs, below) — stored data only, no exchange call
binance-worker prove-performance # reads {"accountId"} JSON from stdin, runs REAL RISC Zero
                           # proving (CPU/RAM-heavy — see "Real ZK performance proof", below)
                           # and prints a portable, independently-verifiable proof
binance-worker scheduler  # long-running: keeps a real-time trade stream open per active
                           # connection, and re-syncs deposits/withdrawals/catalog on an interval
```

Every subcommand takes credentials via **stdin only**, never a CLI
argument — arguments are visible in the process list (`ps aux`) to
anyone on the same machine; stdin is not.

`apps/api/src/binance-connect` spawns `connect`/`status`/`nav` as a
subprocess per HTTP request; `scheduler` is meant to run as its own
long-lived process (see `infra/docker-compose.yml`'s `binance-worker`
service, or run it directly during local development).

## No symbol selection needed for ongoing trades

`myTrades` requires a symbol per call — Binance has no "all trades,
any symbol" REST endpoint — so an early version of this crate required
declaring which pairs to track at connect time. That's gone for
*ongoing* trades: `stream.rs` opens the account's real-time user data
stream (`userDataStream.subscribe.signature`, the HMAC-signed
WebSocket API method — the older `listenKey` REST flow returns `410
Gone` as of 2026-01-21, confirmed against the real API during this
delivery) and receives an `executionReport` event for every fill, on
any symbol, the moment it happens. `run_scheduler` keeps one such
stream task alive per active connection, respawning it if it drops.

`symbols` in the `connect` request is now optional and narrower in
scope: it only controls a one-time *historical* backfill via
`myTrades` at connect time (there is no way to backfill "everything
ever traded" without knowing which pairs to ask about — that remains a
real, documented gap for pre-connection history). Trades from the
moment of connection forward are captured regardless of symbol.

## Environment

- `DATABASE_URL` — a Postgres connection string. Migrations
  (`migrations/`) run automatically on startup.
- `BINANCE_WORKER_ENCRYPTION_KEY` — a 64-character hex string (32
  bytes), e.g. `openssl rand -hex 32`. Encrypts stored API
  keys/secrets at rest (AES-256-GCM). **Known limitation, documented
  not hidden**: this is a single symmetric key from the environment,
  not a real KMS — the same gap `collector_credentials::kms::LocalKms`
  already documents as "not a production KMS." A real deployment wires
  a real KMS here the same way.
- `BINANCE_WORKER_SYNC_INTERVAL_SECONDS` — optional, default `900`
  (15 minutes). Governs deposit/withdrawal/catalog polling only —
  trades are real-time via the stream, independent of this interval.
- `BINANCE_WORKER_A0_SIGNING_KEY` — a 64-character hex string (32
  bytes), e.g. `openssl rand -hex 32`. Only needed for
  `prove-performance`: the collector's own A0 signing key (per
  the protocol specification, "A0 é assinatura de coletor
  identificado" — this worker signs as the identified collector of
  what it gathered, not as Binance itself). Separate from
  `BINANCE_WORKER_ENCRYPTION_KEY`, which protects stored credentials
  at rest rather than being a public-verifiable signing identity.

## What a sync cycle does

Every cycle (the immediate one at `connect` time, and every
`scheduler` tick) re-validates the key is still read-only — a key's
permissions can change after it was first connected, so this is
checked every time, not only once. It fetches any configured
historical `symbols`' trades, the account's full deposit/withdrawal
history, and the current exchange catalog, and persists all of it with
`ON CONFLICT DO NOTHING` upserts — re-syncing never duplicates a trade
or flow already recorded, matching `binance-trades`'/`binance-flows`'
own idempotency guarantees at the storage layer. A failed cycle leaves
`last_synced_at` untouched, so the connection is retried on the next
tick rather than silently marked current.

## Real NAV (`publish.rs`)

`nav` computes a genuine current NAV: fetches the account's real
balance (`GET /api/v3/account`) and a real price mark for every held
asset (`GET /api/v3/klines`, via `crates/domain/valuation` and
`binance-prices`' already-tested reference price policy — a closed
1-minute candle, max 120s old, never a future candle, never a zero
fallback). An asset with no market at all against the consolidated
currency (USDT) — e.g. a fiat balance like BRL — is excluded from the
NAV as out of scope for the spot v0.1 profile, not silently priced; an
asset that *has* a market but genuinely has no current price refuses
the *entire* NAV, matching `value_at_cut`'s own documented "refuse
entirely, never partially" behavior.

**This is a single point-in-time snapshot, not a historical series.**
Returns, Sharpe/Sortino, CAGR, max drawdown and win rate need a real
time series — see `history.rs`, below.

## Real historical NAV, returns and metrics (`history.rs`)

**Everything is measured from the moment the account was connected**
(`binance_connections.created_at`, kept when the same account later
changes credentials so a record can't be restarted by reconnecting).
What the account held at that instant is the *opening balance* — a
starting point, not a gain — and only trades and deposits/withdrawals
from then on move the NAV series, the TWR returns, the Sharpe/Sortino/
drawdown/CAGR, the win rate and the PnL. Anything the exchange reports
from before the connection is deliberately ignored. This is what makes
a track record impossible to cherry-pick after the fact, and it relies
on `binance-worker scheduler` running: it captures every new fill in
real time, on every symbol, from the connection onward.

`pnl.rs` turns those fills and flows into realized/unrealized profit per
position on an average-cost basis (unit-tested rules: openings start
flat, sales realize against the average cost, deposits enter at market
price and withdrawals leave at cost — neither is profit — fees are
reported separately).

`load_and_compute_history` turns the trades/flows this connection has
actually collected into a real daily NAV series, a real TWR index, and
real per-day returns — what `crates/domain/metrics` needs for Sharpe/
Sortino/CAGR/MDD, none of which `publish.rs`'s single snapshot alone
can produce.

**The reconstruction anchor is the real, current balance**
(`GET /api/v3/account`), not an assumption that collected history is
complete from account inception — `valuation::reconstruct_quantity`
walks backward from that anchor through the collected ledger's deltas
to each historical checkpoint. A historical point before this
connection existed is only as accurate as the collected history is;
`HistoryResult::earliest_reliable_checkpoint_ms` names exactly where
that reliability starts, so a caller never treats an earlier point as
trustworthy.

**Deposits/withdrawals are `Flow` events in the TWR timeline, not
folded into the NAV jump** — this was a real bug caught during this
delivery's own homologation: without it, a genuine deposit showed up
as an 86.8%-in-one-day "return," which is exactly what the protocol
("aporte de USD 100.000 sem lucro em carteira de USD 10.000 DEVE
produzir retorno zero") forbids. Each deposit/withdrawal's USDT value
is resolved at its own historical instant and inserted into the
timeline `crates/domain/returns::compute_twr` consumes; trades are
excluded (an internal asset swap, not external capital), and Convert/
Dust/Dividend/Transfer are not yet classified as external-vs-internal
— a real, documented gap, not silently treated as neither.

Tested against this delivery's own real account: ~109 days of real
history reconstructed a real Sharpe (-0.37), Sortino (-0.50) and a
real 24.1% max drawdown on the deposit-excluded TWR index — and CAGR
correctly refused (109 days, needs 365), not fabricated a number for
insufficient history.

**Speed: the price lookups are overlapped, not cached.** Each
historical mark is an independent public `klines` round trip (~0.3s),
and the daily reconstruction needs one per held asset per checkpoint —
done one after another that took ~22–27s for ~30 days and over a
minute for months (~74s for ~109 checkpoints against 2 assets). Those
lookups now run concurrently (`binance_live_client::parallel_map`,
at most 8 in flight, results kept in input order), and the ~17 MB
`exchangeInfo` catalog is no longer downloaded per request — only the
few `symbol=` entries actually needed are (`fetch_symbol_assets_for`).
Measured on the real account: NAV 3.3s → 1.3s, performance
22–27s → ~4s, with identical results (same 32 daily points, same
max drawdown to 28 digits). Nothing is cached, so no number can be
stale; a persistent store of *closed past-day* marks (immutable) would
be the next step for very long histories.

`metrics::compute_win_rate` (FIFO-matched round trips, see that
crate's own doc comment) is wired into `history.rs` (`compute_overall_
win_rate`) and exposed by `performance`/`binance-performance`.

## The executions already collected (`trade_log.rs`)

`trades` hands back the executions this worker has already persisted —
narrowed to one market or one period, a bounded page at a time. It reads
stored rows only: it opens no exchange connection, decrypts no
credential, and computes nothing, so it is the cheapest read here and
the only one that cannot fail on a market call.

Trades are ordered by `(time_ms, symbol, id)`, never `time_ms` alone:
`(symbol, id)` is the namespace of an execution, so two unrelated trades
can share both a millisecond and a numeric id, and a page boundary drawn
on the incomplete key would drop or repeat them. A page's `nextCursor`
carries that whole key, and is handed out only when a further page
actually exists.

Every exchange answers through the same code: Binance's
`binance_synced_trades`, Coinbase's fills and Kraken's trades are all
loaded as `Trade` first. An on-chain wallet and an IBKR Flex statement
carry no executions of their own, so they answer with an empty page —
the true answer for them, not a missing one.

## Real ZK performance proof (`prove.rs`)

`prove-performance` proves a real claim about this account's real
performance — the return and max drawdown over the covered period —
through `zkvm/methods/performance`'s existing performance guest, without
revealing the account's raw trades, flows, balance or identifier to
whoever later verifies the proof. This is the piece that lets someone
share "my return over the last N days is X%" as a real, independently
checkable fact instead of a screenshot someone has to trust.

**Why the guest is fed `HistoryResult::twr_index`, never raw NAV**: the
The performance guest's `reconcile()` is a strict spot check (`sum(ledger_deltas)
== nav.last() - nav.first()`), with no external-flow modeling of its
own (see `zkvm/methods/performance/guest/README.md`'s "Out of scope").
Feeding it this account's raw NAV — which for a real account includes
real deposits — would silently reproduce the exact flow-adjustment bug
`history.rs` already caught and fixed once (a deposit read back as
investment return). `prove.rs` avoids that by feeding the guest the
chained TWR index instead, which `returns::compute_twr` has *already*
factored deposits/withdrawals out of before this module ever sees it;
the guest's own reconciliation then holds trivially, and what it
proves is genuinely "this index evolved this way," not "this balance
changed this way."

The signed `SourceEnvelope`'s commitments (`raw_evidence_root` over
the real collected trades/flows, `normalized_root` over the real TWR
index series, `coverage_manifest_hash`, `account_binding_commitment`)
are built with `crates/commitments` — the same salted, JCS-canonical
hashing the rest of the protocol's commitment scheme uses — so the
committed data is real, even though this delivery does not yet expose
per-record inclusion proofs against them (a real, documented follow-up,
not a silent gap: distinct per-leaf salts and Merkle inclusion proofs
would let a verifier check one specific trade without seeing the rest).
`trust_manifest_hash`/`policy_hash` hash a fixed, documented sentinel
string rather than a real trust-manifest/policy registry, which does
not exist yet either (same "Out of scope" the guest's own README
already names).

The resulting receipt is `bincode`-encoded and hex-printed
(`receiptHex`/`journalHex`) in the exact format
`crates/verifier::receipt::verify_receipt` (and the `linvesther-verify`
CLI) already expect — a verifier checks it entirely offline, against
`zkvm-methods::GUEST_ID`, never trusting this worker or `apps/api`
again. Producing a full portable bundle (`manifest.json`,
`accounts.json`, `checkpoints/index.json` — the format
`linvesther-verify` reads from a directory) is a real, separate,
not-yet-built follow-up; this delivers the proof itself.

**This runs real RISC Zero proving (`risc0_zkvm::default_prover()`,
not dev-mode) — CPU/RAM-heavy and genuinely slow.** `tests/prove_test.rs`
is `#[ignore]`d for exactly this reason and is never part of the
default gate; run it deliberately, not as part of routine `cargo test`.

Real numbers from this delivery's own run, against a real account with
~110 days of history (far more than the guest's own 2-point unit
test): a first, uncapped attempt genuinely **OOM-killed the entire
development machine**, not just the test process — real proving's peak
memory is driven by the zkVM's segment size, not by how small the
guest's own computation looks. The fix that worked was running it
inside a memory-capped, swap-disabled cgroup (so a repeat blow-up kills
only the proving process, never the host) and forcing single-threaded
execution (trading a lot of speed for a much lower peak):

```sh
export RAYON_NUM_THREADS=1
systemd-run --user --scope -p MemoryMax=11G -p MemorySwapMax=0 -- \
  cargo test --manifest-path services/Cargo.toml -p binance-worker \
  --test prove_test -- --ignored --test-threads=1 --nocapture
```

Even contained this way, single-threaded real proving of ~110
checkpoints is genuinely slow: **11,832.98s (3h 17m) end to end** in
this delivery's own real run, against the real, connected account,
with no crash and no stall. The receipt verified for real, against the
real guest image ID: proven return `-73612` (scale 1e6, i.e. -7.36%
over the covered period) and max drawdown `2410` bp (24.10%) — the
drawdown matches this same account's off-chain `history.rs`
reconstruction exactly (documented above as "a real 24.1% max
drawdown"), a real cross-check between the two independent
computations, not just a plausible-looking number. Proving progresses in segments (watch `systemctl --user
status <unit>`'s `Memory:` line rise then drop repeatedly as each
segment completes and frees its buffers; a long *flat* plateau is
normal, not a hang, as long as the process's own CPU time keeps
climbing). There is no resumable
checkpoint with the simple `prover.prove(env, GUEST_ELF)` call this
module uses — killing it mid-run loses all progress; risc0 does expose
lower-level per-segment proving APIs that would allow incremental,
resumable proving, but this module does not use them yet (a real,
documented follow-up, not a silent gap).

## Running it for real

```sh
docker compose -f infra/docker-compose.yml up -d postgres
cd services && cargo build -p binance-worker
export DATABASE_URL="postgresql://linvestherzk:linvestherzk-local-dev-only@localhost:5433/linvestherzk"
export BINANCE_WORKER_ENCRYPTION_KEY=$(openssl rand -hex 32)

# connect (reads a real key from stdin — never hardcode one):
echo '{"accountId":"acc-1","apiKey":"...","apiSecret":"..."}' | ./target/debug/binance-worker connect

# check status:
echo '{"accountId":"acc-1"}' | ./target/debug/binance-worker status

# a real current NAV:
echo '{"accountId":"acc-1"}' | ./target/debug/binance-worker nav

# run the background scheduler (trade stream + periodic sync):
./target/debug/binance-worker scheduler
```

This was run for real during this delivery, against a genuine
read-only account with no symbols declared at all: connected through
the actual HTTP route, the real-time stream picked up the connection
automatically (confirmed via `docker logs` on the containerized
`scheduler` service), and `nav` computed a real NAV from real BTC/
USDT/SOL balances and real market prices (BRL correctly excluded as
out of scope). See `apps/api/src/binance-connect`'s README and
`tests/e2e/binance-connect/binanceConnect.e2e.test.ts`.

## Tests

- `tests/crypto_test.rs` — round-trips, wrong key rejected, tampered
  ciphertext rejected, invalid key format rejected. Pure, no database,
  part of the default gate.
- `src/stream.rs`'s own unit tests — the WebSocket API's signed-payload
  format, `executionReport` field extraction, a null commission asset
  represented honestly as empty rather than fabricated. Pure, part of
  the default gate.
- `tests/trade_log_test.rs` — the order key, the symbol and period
  filters, walking every trade exactly once across pages, the same
  numeric id on two markets kept apart at a page boundary, no cursor
  handed out for a page that is already the last, and a refused cursor
  or page size. Pure, no database, part of the default gate.
- `tests/db_test.rs` — real Postgres integration: connect/find
  round-trips, reconnecting the same account upserts rather than
  duplicates, trade/flow dedup, the `(symbol, id)` namespace rule,
  connection summary reflects persisted data, and the due-for-sync
  query's boundary. `#[ignore]`d (needs a running local Postgres, same
  reason `infra/recovery`'s own gate doesn't bring up
  `docker-compose.yml`).
- `tests/stream_test.rs` — a real WebSocket subscribe against the live
  account, confirming the signed request is accepted (a timeout while
  still listening, rather than an immediate error, is the proof).
  `#[ignore]`d — real credential and network call.
- `tests/publish_test.rs` — a real NAV computation against the live
  account. `#[ignore]`d — real credential and network calls.
- `tests/history_test.rs` — real historical NAV/TWR reconstruction and
  real Sharpe/Sortino/MDD/CAGR against the live account's actual
  collected history. `#[ignore]`d — real credential, network calls,
  and a real (already-connected) account with trade history to
  reconstruct.
- `tests/prove_test.rs` — real RISC Zero proving over the live
  account's real, collected history, then verifies the resulting
  receipt against the real guest image ID. `#[ignore]`d for the same
  reasons as `history_test.rs`, plus: this one is genuinely
  CPU/RAM-heavy (real proving, not dev-mode) — run it deliberately,
  not swept in with the rest.

Run every `#[ignore]`d real test together with:

```sh
docker compose -f infra/docker-compose.yml up -d postgres
set -a && source .env && set +a
export DATABASE_URL="postgresql://linvestherzk:linvestherzk-local-dev-only@localhost:5433/linvestherzk"
cargo test --manifest-path services/Cargo.toml -p binance-worker -p binance-live-client -- --ignored --test-threads=1
```

## What this does not do

No process supervisor/restart policy for the `scheduler` binary beyond
what a real deployment's orchestrator (systemd, Docker restart
policies, etc.) provides. No rate-limit backoff tuned to Binance's
actual weight limits. No pre-connection historical backfill beyond
whatever `symbols` were explicitly named at connect time. No portable
proof bundle (`manifest.json` + friends) from `prove-performance` yet,
only the raw receipt/journal — see "Real ZK performance proof", above.
No per-record inclusion proofs against the envelope's commitment
roots. `apps/api/src/binance-connect` is an HTTP API only; no UI wires
`prove-performance` yet (the dashboard at `apps/web/app/portfolio`
covers `nav`/`performance` only, not proving).
