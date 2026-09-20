use guest::envelope::SourceEnvelope;
use guest::{run, GuestError, GuestInput};
use k256::ecdsa::signature::Signer;
use k256::ecdsa::SigningKey;

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32].into()).unwrap()
}

fn base_envelope() -> SourceEnvelope {
    SourceEnvelope {
        mechanism: "A0".to_string(),
        issuer_key_id: "collector-1".to_string(),
        trust_manifest_hash: [1u8; 32],
        account_binding_commitment: [2u8; 32],
        batch_commitment: [3u8; 32],
        raw_evidence_root: [4u8; 32],
        normalized_root: [5u8; 32],
        coverage_manifest_hash: [6u8; 32],
        period_start_ms: 1_000,
        period_end_ms: 2_000,
        observed_at_ms: 1_500,
        expires_at_ms: 100_000,
        policy_hash: [8u8; 32],
        environment: "production".to_string(),
        source_session_binding: [9u8; 32],
    }
}

const SCALE: i64 = 1_000_000;

fn signed_input(envelope: SourceEnvelope, nav: Vec<i64>, ledger_deltas: Vec<i64>) -> GuestInput {
    let key = signing_key();
    let signature: k256::ecdsa::Signature = key.sign(&envelope.digest());
    GuestInput {
        envelope,
        verifying_key_sec1: key.verifying_key().to_sec1_bytes().to_vec(),
        signature_compact: signature.to_bytes().to_vec(),
        nav,
        ledger_deltas,
    }
}

#[test]
fn a_well_formed_input_yields_a_journal_with_every_field_fixed() {
    let nav = vec![100 * SCALE, 110 * SCALE];
    let deltas = vec![10 * SCALE];
    let input = signed_input(base_envelope(), nav, deltas);

    let output = run(&input).expect("valid signature, reconciled ledger");
    assert_eq!(output.envelope_digest, input.envelope.digest());
    assert_eq!(output.period_start_ms, 1_000);
    assert_eq!(output.period_end_ms, 2_000);
    assert_eq!(output.twr_index_scaled, 1_100_000);
    assert_eq!(output.mdd_bp, 0);
    assert_eq!(output.capital, 110 * SCALE);
}

#[test]
fn a_tampered_envelope_field_invalidates_origin_and_the_guest_refuses() {
    let mut input = signed_input(
        base_envelope(),
        vec![100 * SCALE, 110 * SCALE],
        vec![10 * SCALE],
    );
    // The signature was produced over the original envelope; mutating any
    // field after signing must make the guest reject the input outright.
    input.envelope.batch_commitment = [99u8; 32];

    let result = run(&input);
    assert!(matches!(result, Err(GuestError::Origin(_))));
}

#[test]
fn an_unreconciled_ledger_is_refused_even_with_a_valid_signature() {
    let input = signed_input(
        base_envelope(),
        vec![100 * SCALE, 110 * SCALE],
        vec![5 * SCALE],
    );
    let result = run(&input);
    assert!(matches!(result, Err(GuestError::Compute(_))));
}

#[test]
fn a_signature_from_an_untrusted_key_is_refused() {
    let envelope = base_envelope();
    let other_key = SigningKey::from_bytes(&[11u8; 32].into()).unwrap();
    let signature: k256::ecdsa::Signature = other_key.sign(&envelope.digest());
    // verifying_key_sec1 claims to be the trusted signer, but the
    // signature actually came from a different key.
    let trusted_key = signing_key();
    let input = GuestInput {
        envelope,
        verifying_key_sec1: trusted_key.verifying_key().to_sec1_bytes().to_vec(),
        signature_compact: signature.to_bytes().to_vec(),
        nav: vec![100 * SCALE, 110 * SCALE],
        ledger_deltas: vec![10 * SCALE],
    };
    assert!(matches!(run(&input), Err(GuestError::Origin(_))));
}

/// Fingerprint of the key `[7u8; 32]`. The same vector is asserted in
/// `services/collector/attestation` and `crates/verifier`, so the three
/// implementations of the fingerprint cannot drift apart unnoticed.
const KEY_7_FINGERPRINT: &str = "9c177b47de4c524a7b7754c648e488d3993b12b88930e0d67f9e3bcd01afc134";

/// The journal bytes for [`base_envelope`] over a 100 -> 110 series. The
/// same bytes are decoded in `crates/verifier`; if the journal layout
/// changes, both tests fail together.
const GOLDEN_JOURNAL_HEX: &str = "760000009300000042000000c2000000a6000000690000002600000077000000810000002d000000f700000013000000030000005b0000002500000093000000fb000000f2000000d60000007b000000150000004e000000b400000080000000f200000083000000500000000a0000001c00000056000000a70000002f0000009c000000170000007b00000047000000de0000004c000000520000004a0000007b0000007700000054000000c600000048000000e400000088000000d3000000990000003b00000012000000b80000008900000030000000e0000000d60000007f0000009e0000003b000000cd00000001000000af000000c100000034000000e803000000000000d007000000000000e0c81000000000000000000000000000000000000000000080778e0600000000";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn the_journal_records_which_collector_key_signed() {
    let input = signed_input(base_envelope(), vec![100 * SCALE, 110 * SCALE], vec![10 * SCALE]);
    let output = run(&input).expect("valid signature, reconciled ledger");
    assert_eq!(hex(&output.signer_fingerprint), KEY_7_FINGERPRINT);
}

#[test]
fn a_different_signer_yields_a_different_fingerprint() {
    let other = SigningKey::from_bytes(&[8u8; 32].into()).unwrap();
    let envelope = base_envelope();
    let signature: k256::ecdsa::Signature = other.sign(&envelope.digest());
    let input = GuestInput {
        envelope,
        verifying_key_sec1: other.verifying_key().to_sec1_bytes().to_vec(),
        signature_compact: signature.to_bytes().to_vec(),
        nav: vec![100 * SCALE, 110 * SCALE],
        ledger_deltas: vec![10 * SCALE],
    };
    let output = run(&input).expect("a signature by any well-formed key verifies");
    assert_ne!(hex(&output.signer_fingerprint), KEY_7_FINGERPRINT);
}

#[test]
fn the_fingerprint_does_not_depend_on_how_the_key_was_encoded() {
    use guest::origin::collector_fingerprint;
    let key = signing_key();
    let compressed = k256::ecdsa::VerifyingKey::from_sec1_bytes(key.verifying_key().to_encoded_point(true).as_bytes()).unwrap();
    let uncompressed = k256::ecdsa::VerifyingKey::from_sec1_bytes(key.verifying_key().to_encoded_point(false).as_bytes()).unwrap();
    assert_eq!(collector_fingerprint(&compressed), collector_fingerprint(&uncompressed));
}

#[test]
fn the_journal_layout_matches_the_golden_vector() {
    let input = signed_input(base_envelope(), vec![100 * SCALE, 110 * SCALE], vec![10 * SCALE]);
    let output = run(&input).unwrap();
    let words = risc0_zkvm::serde::to_vec(&output).unwrap();
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    assert_eq!(hex(&bytes), GOLDEN_JOURNAL_HEX);
}
