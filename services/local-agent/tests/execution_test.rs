use local_agent::{
    canonical_result_bytes, canonical_result_digest, execute_locally, LedgerEvent, LocalOnlyInput,
};

fn fixture() -> LocalOnlyInput {
    LocalOnlyInput {
        api_key: "sk-live-super-secret-key".to_string(),
        api_secret: "top-secret-signing-key".to_string(),
        witness_bytes: b"private-witness-blob-never-leaves".to_vec(),
        ledger_events: vec![
            LedgerEvent {
                id: "evt-1".to_string(),
                delta: 100,
            },
            LedgerEvent {
                id: "evt-2".to_string(),
                delta: -30,
            },
        ],
    }
}

#[test]
fn the_same_fixture_produces_the_same_canonical_result_every_time() {
    let result_a = execute_locally(&fixture());
    let result_b = execute_locally(&fixture());
    assert_eq!(result_a, result_b);
    assert_eq!(
        canonical_result_digest(&result_a),
        canonical_result_digest(&result_b)
    );
}

#[test]
fn the_result_reflects_the_ledger_events() {
    let result = execute_locally(&fixture());
    assert_eq!(result.event_count, 2);
    assert_eq!(result.balance, 70);
}

#[test]
fn the_api_key_secret_and_witness_never_appear_in_what_would_be_sent_to_the_operator() {
    let input = fixture();
    let result = execute_locally(&input);
    let wire_bytes = canonical_result_bytes(&result);
    let wire_text = String::from_utf8(wire_bytes).unwrap();

    assert!(!wire_text.contains(&input.api_key));
    assert!(!wire_text.contains(&input.api_secret));
    assert!(!wire_text.contains("private-witness-blob-never-leaves"));
    assert!(!wire_text.contains("api_key"));
    assert!(!wire_text.contains("witness"));
}

#[test]
fn a_different_ledger_produces_a_different_canonical_digest() {
    let mut input = fixture();
    input.ledger_events.push(LedgerEvent {
        id: "evt-3".to_string(),
        delta: 1,
    });
    let result = execute_locally(&input);
    let baseline = execute_locally(&fixture());
    assert_ne!(
        canonical_result_digest(&result),
        canonical_result_digest(&baseline)
    );
}
