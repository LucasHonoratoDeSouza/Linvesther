use commitments::MerkleTree;
use sha2::{Digest, Sha256};
use verifier::verify_account_set_root;

fn expected_root(member_ids: &[String]) -> [u8; 32] {
    let leaves: Vec<[u8; 32]> = member_ids
        .iter()
        .map(|id| Sha256::digest(id.as_bytes()).into())
        .collect();
    MerkleTree::build(leaves).unwrap().root
}

#[test]
fn the_root_over_the_claimed_membership_verifies() {
    let members = vec!["acc-1".to_string(), "acc-2".to_string()];
    let root = expected_root(&members);
    assert!(verify_account_set_root(&root, &members).is_ok());
}

#[test]
fn a_membership_set_inconsistent_with_the_claimed_root_is_rejected() {
    let claimed_root = [0u8; 32];
    let members = vec!["acc-1".to_string(), "acc-2".to_string()];
    assert!(verify_account_set_root(&claimed_root, &members).is_err());
}

#[test]
fn reordering_members_changes_the_root() {
    let members_a = vec!["acc-1".to_string(), "acc-2".to_string()];
    let members_b = vec!["acc-2".to_string(), "acc-1".to_string()];
    let root_a = expected_root(&members_a);

    // The root built for A must not verify against B's differently
    // ordered membership.
    assert!(verify_account_set_root(&root_a, &members_b).is_err());
}
