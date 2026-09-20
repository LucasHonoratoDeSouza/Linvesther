//! `dataCommitment = H("LZK/data/v1", salt32, JCS(data))`.

use crate::canonical::{canonicalize_value, CanonicalError};
use crate::framing::{frame_hash, FramingError};
use serde_json::Value;

pub const DATA_COMMITMENT_TAG: &str = "LZK/data/v1";

#[derive(Debug, thiserror::Error)]
pub enum CommitmentError {
    #[error(transparent)]
    Canonical(#[from] CanonicalError),
    #[error(transparent)]
    Framing(#[from] FramingError),
}

/// Computes a private data commitment: changing the salt, or changing any
/// field of `data` (which changes its canonical encoding), changes the
/// commitment. `data` must already be validated by the caller (this
/// function canonicalizes a trusted `Value`; use
/// [`crate::canonical::canonicalize`] first if `data` originates as raw,
/// untrusted JSON text).
pub fn data_commitment(salt: &[u8; 32], data: &Value) -> Result<[u8; 32], CommitmentError> {
    let canonical = canonicalize_value(data)?;
    Ok(frame_hash(
        DATA_COMMITMENT_TAG,
        &[salt, canonical.as_slice()],
    )?)
}
