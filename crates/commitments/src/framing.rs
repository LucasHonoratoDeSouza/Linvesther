//! Unambiguous domain-separated hashing: `H(tag, fields...)` from
//! the protocol specification:
//!
//! ```text
//! H(tag, fields...) = SHA256(u32be(len(tag)) || utf8(tag) || Σ[u64be(len(field)) || field])
//! ```
//!
//! Length-prefixing every component (rather than, say, joining with a
//! separator byte) is what makes the framing unambiguous: no sequence of
//! tag/field byte lengths can be reinterpreted as a different tag/field
//! split, which a plain concatenation or delimiter-based scheme could
//! allow an adversary to exploit.

use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FramingError {
    #[error("tag length {0} exceeds u32 range")]
    TagTooLong(usize),
    #[error("field length {0} exceeds u64 range")]
    FieldTooLong(usize),
}

/// Computes `H(tag, fields...)`. Rejects (rather than silently truncating)
/// a tag or field whose byte length cannot be represented in the framing's
/// fixed-width length prefix.
pub fn frame_hash(tag: &str, fields: &[&[u8]]) -> Result<[u8; 32], FramingError> {
    let mut hasher = Sha256::new();
    hasher.update(encode_len_u32(tag.len())?);
    hasher.update(tag.as_bytes());
    for field in fields {
        hasher.update(encode_len_u64(field.len())?);
        hasher.update(field);
    }
    Ok(hasher.finalize().into())
}

fn encode_len_u32(len: usize) -> Result<[u8; 4], FramingError> {
    u32::try_from(len)
        .map(u32::to_be_bytes)
        .map_err(|_| FramingError::TagTooLong(len))
}

fn encode_len_u64(len: usize) -> Result<[u8; 8], FramingError> {
    u64::try_from(len)
        .map(u64::to_be_bytes)
        .map_err(|_| FramingError::FieldTooLong(len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_prefix_overflow_is_rejected_not_truncated() {
        // usize::MAX as a byte length can never be produced by real data on
        // any platform reachable here, but the encoder must still refuse it
        // rather than silently truncate the length prefix — this is the
        // "overflow" guard the conformance gate requires, exercised directly on the
        // pure length-encoding function so the test needs no multi-gigabyte
        // allocation.
        #[cfg(target_pointer_width = "64")]
        {
            let oversized = (u32::MAX as usize) + 1;
            assert_eq!(
                encode_len_u32(oversized),
                Err(FramingError::TagTooLong(oversized))
            );
        }
    }

    #[test]
    fn ordinary_length_encodes_as_expected() {
        assert_eq!(encode_len_u32(5).unwrap(), 5u32.to_be_bytes());
        assert_eq!(encode_len_u64(5).unwrap(), 5u64.to_be_bytes());
    }
}
