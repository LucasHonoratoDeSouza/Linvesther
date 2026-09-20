# Binance catalog archiving and symbol universe

Part of the `services/` self-contained Cargo workspace. Implements the
universe-construction logic from the Binance adapter specification's
"Algoritmo de coleta e fechamento" step 4: "Construir universo como união
dos catálogos preservados no período, símbolos observados em streams e
históricos já conhecidos... Símbolo negociado que depois zera continua
obrigatório."

- `archive`: append-only, chronologically ordered store of `exchangeInfo`
  snapshots. Never edits or drops a version; `delisted_symbols()` derives
  which symbols dropped out of the latest snapshot without touching the
  historical record that shows they were once listed.
- `universe`: `SymbolUniverse` unions three legitimate sources — the full
  catalog archive, live stream observations, and known history from prior
  periods — into a monotonic set. There is no method that removes a
  symbol, and no constructor from "current positions" or a
  caller-declared list, matching the spec's explicit prohibition.

## What this does not do

No live call to `exchangeInfo` or a Binance user data stream — this crate
takes already-obtained snapshots/observations as input. No real Binance
account was available in this delivery; see
`experiments/binance-conformance/README.md` and
`services/collector/credentials/README.md` for the same open item this
inherits (real B0 homologation against a live account is still pending).

## Running

```sh
cargo test --manifest-path ../../Cargo.toml -p binance-catalog
# or from the repo root:
make check-integration
```
