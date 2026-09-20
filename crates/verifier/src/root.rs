//! Account-set root recomputation ("root" failure mode), per
//! the EVM adapter specification's `accountSetRoot` binding to a
//! `MembershipEpoch`. Account IDs are public opaque identifiers (per
//! the protocol specification), not private data, so this hashes them
//! directly (no salt) rather than using `commitments::leaf_hash`'s
//! private-record scheme — a real, independently-checkable commitment
//! over public membership, reusing `commitments::MerkleTree` for the
//! tree construction itself.

use commitments::MerkleTree;
use sha2::{Digest, Sha256};

fn member_leaf(account_id: &str) -> [u8; 32] {
    Sha256::digest(account_id.as_bytes()).into()
}

/// Recomputes the Merkle root over `member_ids` (in the order given —
/// order is part of what's committed to) and compares it to
/// `claimed_root`. Any account added, removed or reordered changes the
/// recomputed root, so a bundle claiming a membership set inconsistent
/// with its own `accountSetRoot` fails here.
pub fn verify_account_set_root(
    claimed_root: &[u8; 32],
    member_ids: &[String],
) -> Result<(), commitments::MerkleError> {
    let leaves: Vec<[u8; 32]> = member_ids.iter().map(|id| member_leaf(id)).collect();
    let tree = MerkleTree::build(leaves)?;
    if &tree.root != claimed_root {
        return Err(commitments::MerkleError::RootMismatch);
    }
    Ok(())
}
