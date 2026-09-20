# binance-live-client

The real, credentialed HTTP client that every other Binance collector
crate's README named as missing: `services/collector/binance/{trades,
catalog,flows,prices}` and `services/collector/credentials` all
implement real, tested logic — pagination, normalization, signing,
`apiRestrictions` validation — but none of them makes an actual network
call. This crate is that missing piece: a signed HTTP client that calls
the real Binance API and feeds real responses into those existing,
already-tested parsers.

## The gate: read-only, or refuse

`LiveClient::ensure_read_only` fetches `apiRestrictions` and validates
it via `collector_credentials::validate_read_only` before anything else
is allowed to run — every other method refuses with
`LiveClientError::ReadOnlyNotConfirmed` until this has succeeded. This
is a uniform gate, not a per-endpoint exemption list: even
`fetch_exchange_info` (public market data) is gated, so there is exactly
one place in this crate's control flow that decides "this credential is
safe to use," not several that could each independently get it wrong.

Every request goes through `collector_credentials::signing`'s fixed
host and path allowlist — this crate adds no code path that accepts an
arbitrary URL or path.

## What this session's real homologation run found

Running against a genuine read-only Binance account surfaced two real
gaps neither documentation nor synthetic fixtures had caught:

1. **`apiRestrictions` has more fields than `collector-credentials`
   knew about.** A real response includes `createTime`,
   `enableFixApiTrade`, `enableFixReadOnly`,
   `enablePortfolioMarginTrading` and `enableVanillaOptions` — none of
   which existed in the original field list. `validate_read_only`'s
   fail-closed design caught this correctly (it refused with
   `UnknownFields` rather than silently treating them as safe); the fix
   was to classify each field explicitly in
   `services/collector/credentials/src/restrictions.rs` (three are
   additional trade/margin/options capabilities now checked like every
   other write capability; `enableFixReadOnly` is a read capability and
   `createTime` is informational, neither blocks validation) and add
   regression tests for each.
2. **`GET /api/v3/exchangeInfo` rejects `timestamp`/`signature`
   parameters** (`-1104: Not all sent parameters were read`) — it is a
   genuinely public endpoint, and Binance does not ignore unexpected
   auth parameters on public paths the way it might be assumed to. This
   crate's `public_get` sends no such parameters for paths that don't
   need them, while still enforcing the read-only gate and the path
   allowlist.

Both are now fixed, tested, and re-verified against the real account —
exactly the kind of gap real homologation exists to find that fixture-
only testing cannot.

## Running the real homologation tests

Every test in `tests/homologation_test.rs` is `#[ignore]`d: it requires
a real `BINANCE_API_KEY`/`BINANCE_API_SECRET` (read-only permissions
only — the client refuses anything else) and makes real network calls,
so it must never run as part of `make check-integration` or any other
default gate.

```sh
set -a && source .env && set +a
cargo test --manifest-path services/Cargo.toml -p binance-live-client -- --ignored --test-threads=1
```

They were run manually during this delivery and passed: the key was
confirmed read-only, every call before that confirmation was refused,
real `exchangeInfo` parsed into a non-empty `CatalogSnapshot`
containing `BTCUSDT`, real `myTrades` parsed without error, and real
deposit/withdrawal history normalized without an unrecognized-status
error.

## What this does not close

This is real connectivity and real parsing against a real account —
Gate B0's "conector Binance real homologado" is now genuinely, not
just synthetically, exercised. It is not a production collector daemon
(no persistence, no scheduling, no retry/backoff policy, no streaming
websocket support) and the specific account tested had limited trade/
deposit/withdrawal history, so not every code path in `binance-flows`
(e.g. every `Convert`/`Dust`/`Dividend` status code) was exercised by a
real record — only what the account actually had.
