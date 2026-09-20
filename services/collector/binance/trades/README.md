# Binance `myTrades` pagination

Part of the `services/` self-contained Cargo workspace. Implements step 5
of the Binance adapter specification's "Algoritmo de coleta e fechamento":
per-symbol `myTrades` pagination exhaustion and namespace-safe storage.

- `pagination`: a short/empty page proves exhaustion; a full page never
  does by itself (`PAGINATION_UNPROVEN` until a terminating page is
  observed) — matches the spec's "Página cheia exige continuação/
  subdivisão; limite truncado sem evidência de término é
  `PAGINATION_UNPROVEN`."
- `store`: `TradeStore` keys trades first by symbol, then by id, so the
  same numeric id under two different symbols can never collide — a
  structural guarantee, not a convention. Re-ingesting an identical trade
  (a retry) is a no-op; ingesting a *different* trade under an
  already-recorded id is rejected and the original is kept, with
  the whole page refused rather than partially applied.

## What this does not do

No live call to `myTrades` — this crate takes already-fetched pages as
input. No real Binance account was available in this delivery; see
`experiments/binance-conformance/README.md` for the same open item.

## Running

```sh
cargo test --manifest-path ../../../Cargo.toml -p binance-trades
# or from the repo root:
make check-integration
```
