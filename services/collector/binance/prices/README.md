# Binance authenticated valuation marks

Part of the `services/` self-contained Cargo workspace. Implements the
reference price policy from the protocol specification: "close da
última vela de 1 minuto já encerrada no instante avaliado, idade máxima
120 s. Não usar vela futura... Sem preço válido, valuation é indisponível;
preço zero não é fallback."

`resolve_mark`/`mark_from_series` have no code path that returns a `Mark`
with an estimated, zero, or non-authentic price — every rejection
(missing candle, unclosed candle, future candle, stale candle) is a typed
`MarkError`, enforced by the function's control flow having no such
branch, not by a runtime guard that could be bypassed.

## What this does not do

No live call to `klines` — this crate resolves marks from already-fetched
candle data. No real Binance account was available in this delivery; see
`experiments/binance-conformance/README.md` for the same open item.

## Running

```sh
cargo test --manifest-path ../../../Cargo.toml -p binance-prices
# or from the repo root:
make check-integration
```
