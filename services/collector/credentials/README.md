# Binance credential vault and connection primitives

Self-contained Rust crate (own workspace: `services/Cargo.toml`, same
pattern as `experiments/*`). Implements the pieces of the Binance adapter specification's "cofre e conexão" that don't require a live account to
validate correctly:

- **`vault`**: envelope-encrypted credential storage (AES-256-GCM,
  per-credential DEK wrapped by a pluggable [`Kms`](src/kms.rs) trait) with
  a revoke-then-purge lifecycle — revocation blocks access immediately,
  the secret material itself is destroyed once 24h past revocation.
- **`restrictions`**: validates `apiRestrictions` is read-only, failing
  closed on any unrecognized permission field rather than assuming it's
  safe.
- **`binding`**: pins a credential's UID/sub-account/environment on first
  use and rejects any later mismatch (a UID change is a different
  account, not a rotation).
- **`signing`**: HMAC-SHA256 request signing confined to a fixed host per
  environment and an explicit endpoint-path allowlist — there is no code
  path that accepts an arbitrary URL.

## What this does not do

This crate has no live HTTP client itself — see
`services/collector/binance/live-client` for the real, credentialed
client that reuses `signing`/`restrictions` from here. Most tests in
this crate still use synthetic fixtures (a `LocalKms` in-memory KEK
stand-in, hardcoded UID/environment contexts), but `restrictions.rs`'s
field list has now been validated against a real `apiRestrictions`
response — see `live-client`'s README for what that run found (four
permission fields this crate didn't originally know about) and fixed.

`LocalKms` is explicitly not a production KMS (see its doc comment): a
real deployment implements the `Kms` trait against an actual cloud KMS.
Persistence is in-memory (`HashMap`); a real deployment backs
`CredentialVault` with a database, which is a separate concern from the
envelope-encryption and lifecycle logic this crate owns.

`LocalKms` is explicitly not a production KMS (see its doc comment): a
real deployment implements the `Kms` trait against an actual cloud KMS.
Persistence is in-memory (`HashMap`); a real deployment backs
`CredentialVault` with a database, which is a separate concern from the
envelope-encryption and lifecycle logic this crate owns.

## Running

```sh
cargo test --manifest-path Cargo.toml
# or from the repo root:
make check-integration
```
