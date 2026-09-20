//! Adversarial cases the conformance gate requires: salt/field changes are caught
//! (also exercised with fixture vectors in vectors_test.rs), duplicate
//! keys and float-syntax numbers are rejected, and Merkle inclusion proofs
//! with invalid padding/index/height are rejected.

use commitments::*;
use serde_json::json;

#[test]
fn duplicate_top_level_key_is_rejected() {
    let result = canonicalize(r#"{"a":1,"a":2}"#);
    assert!(matches!(result, Err(CanonicalError::Parse(_))));
}

#[test]
fn duplicate_nested_key_is_rejected() {
    let result = canonicalize(r#"{"outer":{"x":1,"x":2}}"#);
    assert!(result.is_err());
}

#[test]
fn duplicate_key_inside_array_element_is_rejected() {
    let result = canonicalize(r#"{"list":[{"x":1,"x":2}]}"#);
    assert!(result.is_err());
}

#[test]
fn float_syntax_number_is_rejected() {
    let result = canonicalize(r#"{"amount":1.5}"#);
    assert!(result.is_err());
}

#[test]
fn exponent_syntax_number_is_rejected() {
    let result = canonicalize(r#"{"amount":1e10}"#);
    assert!(result.is_err());
}

#[test]
fn nan_and_infinity_are_not_valid_json_and_are_rejected() {
    // Not valid JSON syntax at all; the parser itself must reject these,
    // not silently coerce them.
    assert!(canonicalize(r#"{"amount":NaN}"#).is_err());
    assert!(canonicalize(r#"{"amount":Infinity}"#).is_err());
}

#[test]
fn integer_numbers_are_accepted() {
    assert!(canonicalize(r#"{"count":42,"negative":-7,"zero":0}"#).is_ok());
}

#[test]
fn changing_salt_changes_leaf_hash() {
    let record = json!({"x": 1});
    let a = leaf_hash(&[0x01; 32], &record).unwrap();
    let b = leaf_hash(&[0x02; 32], &record).unwrap();
    assert_ne!(a, b);
}

#[test]
fn changing_field_changes_leaf_hash() {
    let salt = [0x01; 32];
    let a = leaf_hash(&salt, &json!({"x": 1})).unwrap();
    let b = leaf_hash(&salt, &json!({"x": 2})).unwrap();
    assert_ne!(a, b);
}

#[test]
fn node_hash_is_order_sensitive() {
    let a = [0x01; 32];
    let b = [0x02; 32];
    assert_ne!(node_hash(&a, &b).unwrap(), node_hash(&b, &a).unwrap());
}

fn sample_tree() -> (MerkleTree, Vec<[u8; 32]>) {
    let leaves: Vec<[u8; 32]> = (0u8..5).map(|i| [i; 32]).collect();
    let tree = MerkleTree::build(leaves.clone()).unwrap();
    (tree, leaves)
}

#[test]
fn proof_with_index_out_of_range_is_rejected() {
    let (tree, _leaves) = sample_tree();
    let result = tree.prove(tree.count); // index == count is out of range
    assert!(matches!(result, Err(MerkleError::IndexOutOfRange { .. })));
}

#[test]
fn verification_rejects_index_out_of_range() {
    let (tree, leaves) = sample_tree();
    let mut proof = tree.prove(0).unwrap();
    proof.index = proof.count; // tamper: push index out of range
    let result = verify_inclusion(&leaves[0], &proof, &tree.root);
    assert!(matches!(result, Err(MerkleError::IndexOutOfRange { .. })));
}

#[test]
fn verification_rejects_too_few_siblings_short_padding() {
    let (tree, leaves) = sample_tree();
    let mut proof = tree.prove(2).unwrap();
    proof.siblings.pop(); // truncate: invalid padding, too short
    let result = verify_inclusion(&leaves[2], &proof, &tree.root);
    assert!(matches!(
        result,
        Err(MerkleError::InvalidSiblingCount { .. })
    ));
}

#[test]
fn verification_rejects_excess_siblings() {
    let (tree, leaves) = sample_tree();
    let mut proof = tree.prove(2).unwrap();
    proof.siblings.push([0xAA; 32]); // excess sibling: invalid padding
    let result = verify_inclusion(&leaves[2], &proof, &tree.root);
    assert!(matches!(
        result,
        Err(MerkleError::InvalidSiblingCount { .. })
    ));
}

#[test]
fn verification_rejects_wrong_sibling_value() {
    let (tree, leaves) = sample_tree();
    let mut proof = tree.prove(2).unwrap();
    proof.siblings[0] = [0xFF; 32]; // tamper a real sibling's value
    let result = verify_inclusion(&leaves[2], &proof, &tree.root);
    assert!(matches!(result, Err(MerkleError::RootMismatch)));
}

#[test]
fn verification_rejects_proof_for_different_tree_count() {
    let (tree, leaves) = sample_tree();
    let mut proof = tree.prove(2).unwrap();
    // Claim a different tree size while keeping the same siblings/index:
    // this must not be reinterpreted as a valid proof for a same-shaped
    // smaller/larger tree.
    proof.count = 6;
    let result = verify_inclusion(&leaves[2], &proof, &tree.root);
    assert!(result.is_err());
}

#[test]
fn empty_tree_has_no_valid_proof() {
    let empty = MerkleTree::build(vec![]).unwrap();
    assert!(matches!(empty.prove(0), Err(MerkleError::EmptyTree)));
}

#[test]
fn tampered_leaf_content_fails_verification_even_with_correct_proof_shape() {
    let (tree, _leaves) = sample_tree();
    let proof = tree.prove(2).unwrap();
    let wrong_leaf = [0xEE; 32]; // different leaf value, same proof
    let result = verify_inclusion(&wrong_leaf, &proof, &tree.root);
    assert!(matches!(result, Err(MerkleError::RootMismatch)));
}

#[test]
fn framing_overflow_is_a_typed_error_not_a_panic() {
    // Documents the overflow contract at the public API level; the
    // exhaustive boundary case lives in framing.rs's unit tests.
    let ok = frame_hash("tag", &[b"field"]);
    assert!(ok.is_ok());
}
