# zkvm-methods-tls-origin (guest)

TLS origin composition guest, per the protocol specification's
"Composição sem lacuna de confiança" section and the acceptance criteria:
swapping raw TLS for a different normalized input is detected; a
merely-attested bridge can never earn the A1 label end-to-end.

## Design

`guest/src/lib.rs::run` is structured so the badge it returns is
*computed*, never echoed from an input field:

```rust
pub fn run(input: &GuestInput) -> Result<GuestOutput, GuestError> {
    if !input.origin_verified_in_guest {
        // No session checks run at all. Always BridgeAttested.
        return Ok(GuestOutput { badge: OriginBadge::BridgeAttested, .. });
    }
    // Real, in-guest checks — only this path can reach A1.
    verify::verify_session_shape(...)?;
    verify::verify_wallet_binding(...)?;
    verify::verify_raw_bound_to_input(...)?;  // the raw/normalized swap check
    Ok(GuestOutput { badge: OriginBadge::A1, .. })
}
```

There is no third path: either the guest itself re-derives and checks
the TLS session (reusing the same logic as `services/collector/tlsn`,
duplicated per the cross-workspace pattern used by the performance guest), or
it takes the bridge branch and the badge is structurally
`BridgeAttested` — no receipt content, no flag combination, nothing the
caller supplies can make the bridge branch emit `A1`.

## Tests

`tests/guest_test.rs`:

- `execution_detects_a_normalized_input_swap_without_proving` — the
  guest panics (no journal committed) when `normalized_input_bytes`
  doesn't hash to what the session captured.
- `execution_of_a_bridge_path_commits_bridge_attested_without_proving`
  — a bridge path always succeeds, and its journal always shows
  `BridgeAttested`.
- `a_real_proof_of_in_guest_verification_earns_a1_for_real` /
  `a_real_proof_of_a_bridge_path_never_yields_a1` — the same two
  invariants above, through **real** RISC Zero proving (not dev-mode).
  Both marked `#[ignore]` for the same reason as `crates/verifier`'s
  real-proving test: proving needs more free RAM than a typical desktop
  session reliably has alongside it. Run explicitly with
  `cargo test -p zkvm-methods-tls-origin --test guest_test -- --ignored --test-threads=1`
  when the machine has headroom.

## Out of scope

Real TLSNotary integration (see `services/collector/tlsn`'s README for
the same POC scoping on the host side) — this guest consumes a session
receipt *shaped* like a real TLSNotary output would be, without this
repository actually running TLSNotary against Binance.
