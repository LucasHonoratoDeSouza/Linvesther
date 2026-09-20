use collector_attestation::{verify, A0Signer, SourceEnvelope};

fn signer() -> A0Signer {
    A0Signer::from_bytes(&[7u8; 32]).unwrap()
}

fn envelope() -> SourceEnvelope {
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

#[test]
fn a_signature_verifies_against_its_own_envelope() {
    let signer = signer();
    let envelope = envelope();
    let signature = signer.sign(&envelope);
    assert!(verify(&signer.verifying_key(), &envelope, &signature));
}

#[test]
fn changing_a_commitment_field_invalidates_the_signature() {
    let signer = signer();
    let envelope = envelope();
    let signature = signer.sign(&envelope);

    let mut tampered = envelope.clone();
    tampered.batch_commitment = [99u8; 32];
    assert!(!verify(&signer.verifying_key(), &tampered, &signature));
}

#[test]
fn changing_the_policy_hash_invalidates_the_signature() {
    let signer = signer();
    let envelope = envelope();
    let signature = signer.sign(&envelope);

    let mut tampered = envelope.clone();
    tampered.policy_hash = [99u8; 32];
    assert!(!verify(&signer.verifying_key(), &tampered, &signature));
}

#[test]
fn changing_the_source_session_binding_invalidates_the_signature() {
    let signer = signer();
    let envelope = envelope();
    let signature = signer.sign(&envelope);

    let mut tampered = envelope.clone();
    tampered.source_session_binding = [99u8; 32];
    assert!(!verify(&signer.verifying_key(), &tampered, &signature));
}

#[test]
fn changing_the_interval_invalidates_the_signature() {
    let signer = signer();
    let envelope = envelope();
    let signature = signer.sign(&envelope);

    let mut tampered = envelope.clone();
    tampered.period_end_ms = 3_000;
    assert!(!verify(&signer.verifying_key(), &tampered, &signature));
}

#[test]
fn a_signature_from_a_different_key_does_not_verify() {
    let envelope = envelope();
    let signature = signer().sign(&envelope);
    let other_signer = A0Signer::from_bytes(&[11u8; 32]).unwrap();
    assert!(!verify(
        &other_signer.verifying_key(),
        &envelope,
        &signature
    ));
}

/// The same vector is asserted by the performance guest and the verifier,
/// so the three implementations of the fingerprint cannot drift apart.
#[test]
fn the_collector_fingerprint_matches_the_shared_vector() {
    let signer = A0Signer::from_bytes(&[7u8; 32]).unwrap();
    let hex: String = signer.fingerprint().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        hex,
        "9c177b47de4c524a7b7754c648e488d3993b12b88930e0d67f9e3bcd01afc134"
    );
}

#[test]
fn different_keys_have_different_fingerprints() {
    let a = A0Signer::from_bytes(&[7u8; 32]).unwrap();
    let b = A0Signer::from_bytes(&[8u8; 32]).unwrap();
    assert_ne!(a.fingerprint(), b.fingerprint());
}
