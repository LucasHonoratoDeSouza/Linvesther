# collector-tlsn

TLS origin (A1) homologation POC, per the protocol specification's
A1 requirements.

## This is a POC, not a homologation

the acceptance criteria asks this POC to "registrar limites" (record its
limits), and the risk is that TLSNotary plus a guest may be expensive or
incompatible, so origin and composition are validated separately, with
gates taking precedence over apparent speed. Consistent with that, this crate does **not** integrate an
actual TLSNotary client, does not open a real TLS session to Binance,
and does not produce a cryptographic proof of a live network exchange.
It models and tests the **verification logic** a real session receipt
would need to satisfy — the same honest-fixture pattern every
`services/collector/binance/*` crate already uses for the A0 side.

## Scope

- `session::TlsSessionReceipt` — the fields A1 needs disclosed per
  proofs.md (hostname, request method/path/query, response status and
  body hash, which categories were disclosed, and a wallet-binding
  signature).
- `verify::verify_session_shape` — hostname compatibility and that every
  category proofs.md forbids redacting (wallet selector, symbol, filter,
  type, time window, error, relevant count) is actually present in
  `disclosed_fields`.
- `verify::verify_wallet_binding` — the signature covers the receipt's
  own digest; any field changed after signing (including un-disclosing
  a category) invalidates the binding.
- `verify::verify_raw_bound_to_input` — the literal "falha se raw não é
  ligado ao input" requirement: hashes the bytes a downstream
  calculation actually used and requires an exact match against the
  session's captured response hash.

## Tests

`tests/session_test.rs`: a well-formed session passes every check;
wrong hostname rejected; each of the seven required disclosure
categories individually rejected when redacted; a field changed after
signing invalidates wallet binding; an untrusted signer rejected; and
substituted input bytes rejected by `verify_raw_bound_to_input`.

## Out of scope

No real TLSNotary integration, no real Binance session capture, no
notary-server interaction, no MPC/garbled-circuit TLS proof. `zkvm/methods/tls-origin` is where an actual guest composition, if
pursued, would consume receipts shaped like this one.
