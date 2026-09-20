//! Mirror verification, per the acceptance criteria: "objetos verificáveis em
//! mirror" — a primary object's content and its mirror copy must hash
//! identically; this is checked from the actual bytes of each, not
//! trusted from a claimed hash alone.

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorMismatch {
    pub primary_hash: String,
    pub mirror_hash: String,
}

fn sha256_hex(content: &[u8]) -> String {
    hex::encode(Sha256::digest(content))
}

/// Hashes both `primary` and `mirror` and compares them — a mirror that
/// silently diverged from its primary (partial sync, corruption, stale
/// copy) is caught here, not assumed correct because the mirror exists.
pub fn verify_object_mirrored(primary: &[u8], mirror: &[u8]) -> Result<(), MirrorMismatch> {
    let primary_hash = sha256_hex(primary);
    let mirror_hash = sha256_hex(mirror);
    if primary_hash != mirror_hash {
        return Err(MirrorMismatch {
            primary_hash,
            mirror_hash,
        });
    }
    Ok(())
}
