use collector_attestation::SourceEnvelope;

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

#[test]
fn identical_envelopes_produce_the_same_digest() {
    assert_eq!(base_envelope().digest(), base_envelope().digest());
}

#[test]
fn a_string_boundary_shift_does_not_collide() {
    // "collector-1" + "" vs "collector" + "-1", concatenated naively,
    // would produce identical bytes without length-prefixing.
    let mut a = base_envelope();
    a.issuer_key_id = "collector-1".to_string();
    a.environment = "".to_string();

    let mut b = base_envelope();
    b.issuer_key_id = "collector".to_string();
    b.environment = "-1".to_string();

    assert_ne!(a.digest(), b.digest());
}

#[test]
fn every_field_change_produces_a_different_digest() {
    let base = base_envelope();
    let base_digest = base.digest();

    let mutations: Vec<SourceEnvelope> = vec![
        SourceEnvelope {
            mechanism: "A1".to_string(),
            ..base.clone()
        },
        SourceEnvelope {
            issuer_key_id: "collector-2".to_string(),
            ..base.clone()
        },
        SourceEnvelope {
            trust_manifest_hash: [99u8; 32],
            ..base.clone()
        },
        SourceEnvelope {
            account_binding_commitment: [99u8; 32],
            ..base.clone()
        },
        SourceEnvelope {
            batch_commitment: [99u8; 32],
            ..base.clone()
        },
        SourceEnvelope {
            raw_evidence_root: [99u8; 32],
            ..base.clone()
        },
        SourceEnvelope {
            normalized_root: [99u8; 32],
            ..base.clone()
        },
        SourceEnvelope {
            coverage_manifest_hash: [99u8; 32],
            ..base.clone()
        },
        SourceEnvelope {
            period_start_ms: 1_001,
            ..base.clone()
        },
        SourceEnvelope {
            period_end_ms: 2_001,
            ..base.clone()
        },
        SourceEnvelope {
            observed_at_ms: 1_501,
            ..base.clone()
        },
        SourceEnvelope {
            expires_at_ms: 100_001,
            ..base.clone()
        },
        SourceEnvelope {
            policy_hash: [99u8; 32],
            ..base.clone()
        },
        SourceEnvelope {
            environment: "staging".to_string(),
            ..base.clone()
        },
        SourceEnvelope {
            source_session_binding: [99u8; 32],
            ..base.clone()
        },
    ];

    for mutated in mutations {
        assert_ne!(
            mutated.digest(),
            base_digest,
            "mutation did not change the digest: {mutated:?}"
        );
    }
}
