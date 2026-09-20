# Binance flow normalization

Part of the `services/` self-contained Cargo workspace. Implements
the Binance adapter specification's six balance-affecting endpoint families —
deposit, withdrawal, universal transfer, Convert, dust, dividend — each
normalizing into the same `NormalizedFlow` shape (`common.rs`).

Every family fails closed on an unrecognized status/type/reason rather
than guessing, per the spec ("Não reconhecer... tipo desconhecido bloqueia
valuation"). Convert is modeled as an internal swap (two legs: debit
source, credit destination), never a fictitious deposit/withdrawal, per
the spec's explicit instruction. A fee, where a family charges one, is
always its own field — never folded into or duplicated across the
principal legs.

## Policy inputs this crate deliberately does not hardcode

`transfer::TransferTypePolicy` and `dividend::DividendReasonPolicy` are
supplied by the caller rather than baked in: which universal-transfer type
codes actually cross the Spot perimeter, and which dividend reason codes
are homologated, are operational decisions per
the Binance adapter specification ("Enumerar tipos permitidos na política",
"Classificar por motivo homologado") — not facts this crate should assert
from memory of Binance's API surface. A real deployment's policy config
populates these; nothing here invents Binance's actual type/reason
vocabulary.

## What this does not do

No live HTTP calls — this crate normalizes already-fetched records. No
real Binance account was available in this delivery; see
`experiments/binance-conformance/README.md` for the same open item.
Reconciling a whole ledger (summing legs across many flows and trades into
per-asset balances) is `crates/domain/ledger`'s job, not this
crate's — this crate's "reconciled" scope stops at each individual record
having fee and principal correctly, separately represented.

## Running

```sh
cargo test --manifest-path ../../../Cargo.toml -p binance-flows
# or from the repo root:
make check-integration
```
