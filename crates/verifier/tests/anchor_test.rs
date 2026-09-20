use std::collections::HashSet;
use verifier::{verify_anchor, TrustedAnchor};

#[test]
fn an_anchor_present_in_the_trusted_set_verifies() {
    let anchor = TrustedAnchor {
        tx_hash: "0xabc".to_string(),
        block_number: 10,
    };
    let trusted: HashSet<TrustedAnchor> = [anchor.clone()].into_iter().collect();
    assert!(verify_anchor(&anchor, &trusted).is_ok());
}

#[test]
fn an_anchor_not_independently_confirmed_is_rejected() {
    let claimed = TrustedAnchor {
        tx_hash: "0xforged".to_string(),
        block_number: 999,
    };
    let trusted: HashSet<TrustedAnchor> = [TrustedAnchor {
        tx_hash: "0xabc".to_string(),
        block_number: 10,
    }]
    .into_iter()
    .collect();
    assert!(verify_anchor(&claimed, &trusted).is_err());
}

#[test]
fn a_matching_tx_hash_at_the_wrong_block_is_rejected() {
    let claimed = TrustedAnchor {
        tx_hash: "0xabc".to_string(),
        block_number: 11,
    };
    let trusted: HashSet<TrustedAnchor> = [TrustedAnchor {
        tx_hash: "0xabc".to_string(),
        block_number: 10,
    }]
    .into_iter()
    .collect();
    assert!(verify_anchor(&claimed, &trusted).is_err());
}
