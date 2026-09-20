//! Merkle tree per the protocol specification:
//!
//! - leaf: `H("LZK/leaf/v1", salt32, JCS(record))`
//! - node: `H("LZK/node/v1", left32, right32)`, child order preserved
//! - odd-sized level: duplicate the last node to pair it with itself
//! - external root: `H("LZK/tree/v1", u64be(count), top32)`
//! - empty tree: `top32 = H("LZK/empty/v1")`, `count = 0`

use crate::canonical::{canonicalize_value, CanonicalError};
use crate::framing::{frame_hash, FramingError};
use serde_json::Value;

pub const LEAF_TAG: &str = "LZK/leaf/v1";
pub const NODE_TAG: &str = "LZK/node/v1";
pub const TREE_TAG: &str = "LZK/tree/v1";
pub const EMPTY_TAG: &str = "LZK/empty/v1";

#[derive(Debug, thiserror::Error)]
pub enum MerkleError {
    #[error(transparent)]
    Canonical(#[from] CanonicalError),
    #[error(transparent)]
    Framing(#[from] FramingError),
    #[error("index {index} out of range for count {count}")]
    IndexOutOfRange { index: u64, count: u64 },
    #[error("proof carries {actual} siblings, expected exactly {expected} for this tree height")]
    InvalidSiblingCount { expected: usize, actual: usize },
    #[error("recomputed root does not match the expected root")]
    RootMismatch,
    #[error("cannot prove inclusion in an empty tree")]
    EmptyTree,
}

/// Hashes one private leaf: `H("LZK/leaf/v1", salt32, JCS(record))`.
pub fn leaf_hash(salt: &[u8; 32], record: &Value) -> Result<[u8; 32], MerkleError> {
    let canonical = canonicalize_value(record)?;
    Ok(frame_hash(LEAF_TAG, &[salt, canonical.as_slice()])?)
}

/// Hashes one internal node: `H("LZK/node/v1", left32, right32)`. Child
/// order is preserved by construction — this function never sorts its
/// arguments, so swapping `left`/`right` changes the result.
pub fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> Result<[u8; 32], MerkleError> {
    Ok(frame_hash(NODE_TAG, &[left, right])?)
}

fn empty_top() -> Result<[u8; 32], MerkleError> {
    Ok(frame_hash(EMPTY_TAG, &[])?)
}

fn external_root(count: u64, top: &[u8; 32]) -> Result<[u8; 32], MerkleError> {
    Ok(frame_hash(TREE_TAG, &[&count.to_be_bytes(), top])?)
}

/// One inclusion proof: the leaf's position, the tree's total leaf count,
/// and the sibling hash needed at each level from leaf to root, in
/// bottom-to-top order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InclusionProof {
    pub index: u64,
    pub count: u64,
    pub siblings: Vec<[u8; 32]>,
}

/// A built Merkle tree over already-hashed leaves.
pub struct MerkleTree {
    pub root: [u8; 32],
    pub count: u64,
    levels: Vec<Vec<[u8; 32]>>,
}

impl MerkleTree {
    /// Builds a tree over `leaves` (already leaf-hashed; this type does not
    /// hold the private salts/records needed to recompute leaf hashes).
    pub fn build(leaves: Vec<[u8; 32]>) -> Result<Self, MerkleError> {
        if leaves.is_empty() {
            let top = empty_top()?;
            return Ok(Self {
                root: external_root(0, &top)?,
                count: 0,
                levels: vec![],
            });
        }
        let count = leaves.len() as u64;
        let mut levels = vec![leaves];
        while levels.last().expect("levels is never empty here").len() > 1 {
            let next = reduce_level(levels.last().unwrap());
            levels.push(next);
        }
        let top = levels.last().unwrap()[0];
        Ok(Self {
            root: external_root(count, &top)?,
            count,
            levels,
        })
    }

    /// The number of sibling hashes any valid proof over this tree must
    /// carry — one per reduction level, `0` for a single-leaf tree.
    pub fn height(&self) -> usize {
        self.levels.len().saturating_sub(1)
    }

    pub fn prove(&self, index: u64) -> Result<InclusionProof, MerkleError> {
        if self.count == 0 {
            return Err(MerkleError::EmptyTree);
        }
        if index >= self.count {
            return Err(MerkleError::IndexOutOfRange {
                index,
                count: self.count,
            });
        }
        let mut siblings = Vec::with_capacity(self.height());
        let mut i = index as usize;
        for level in &self.levels[..self.levels.len() - 1] {
            let sibling_index = if i.is_multiple_of(2) {
                if i + 1 < level.len() {
                    i + 1
                } else {
                    i // odd-sized level: last node was duplicated to pair with itself
                }
            } else {
                i - 1
            };
            siblings.push(level[sibling_index]);
            i /= 2;
        }
        Ok(InclusionProof {
            index,
            count: self.count,
            siblings,
        })
    }
}

fn reduce_level(level: &[[u8; 32]]) -> Vec<[u8; 32]> {
    let mut next = Vec::with_capacity(level.len().div_ceil(2));
    let mut i = 0;
    while i < level.len() {
        let left = level[i];
        let right = level.get(i + 1).copied().unwrap_or(left); // duplicate last on odd count
        next.push(node_hash(&left, &right).expect("node_hash cannot fail for fixed-size fields"));
        i += 2;
    }
    next
}

fn expected_height(count: u64) -> usize {
    if count <= 1 {
        return 0;
    }
    let mut height = 0;
    let mut n = count;
    while n > 1 {
        n = n.div_ceil(2);
        height += 1;
    }
    height
}

/// Verifies that `leaf` is included at `proof.index` of a tree of
/// `proof.count` leaves whose external root is `expected_root`. Rejects an
/// index outside `count`, a sibling list of the wrong length for the
/// tree's height (whether short or padded with excess entries), and any
/// recomputed root that does not match.
pub fn verify_inclusion(
    leaf: &[u8; 32],
    proof: &InclusionProof,
    expected_root: &[u8; 32],
) -> Result<(), MerkleError> {
    if proof.count == 0 {
        return Err(MerkleError::EmptyTree);
    }
    if proof.index >= proof.count {
        return Err(MerkleError::IndexOutOfRange {
            index: proof.index,
            count: proof.count,
        });
    }
    let height = expected_height(proof.count);
    if proof.siblings.len() != height {
        return Err(MerkleError::InvalidSiblingCount {
            expected: height,
            actual: proof.siblings.len(),
        });
    }

    let mut hash = *leaf;
    let mut index = proof.index;
    for sibling in &proof.siblings {
        hash = if index.is_multiple_of(2) {
            node_hash(&hash, sibling)?
        } else {
            node_hash(sibling, &hash)?
        };
        index /= 2;
    }
    let recomputed = external_root(proof.count, &hash)?;
    if &recomputed != expected_root {
        return Err(MerkleError::RootMismatch);
    }
    Ok(())
}
