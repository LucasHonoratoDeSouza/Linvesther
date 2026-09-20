//! Codecs and commitments for suite `LZK-JCS-SHA256-v1`
//! (the protocol specification).

pub mod canonical;
pub mod commitment;
pub mod framing;
pub mod merkle;
pub mod salt;

pub use canonical::{canonicalize, canonicalize_value, CanonicalError};
pub use commitment::{data_commitment, CommitmentError};
pub use framing::{frame_hash, FramingError};
pub use merkle::{leaf_hash, node_hash, verify_inclusion, InclusionProof, MerkleError, MerkleTree};
pub use salt::{generate_salt, SaltError};
