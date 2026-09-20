//! Version chain for a corrected object, per the protocol specification:
//! `Correction`'s definition is "objeto atingido, motivo, evidência e
//! versão substituta; nunca alteração in-place."

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionStatus {
    Original,
    Superseded { by: [u8; 32] },
    Disputed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub digest: [u8; 32],
    pub status: VersionStatus,
}

/// The outcome of applying an admissible correction: the original version
/// is returned alongside the replacement, never dropped or edited. Its
/// `digest` is byte-for-byte the digest passed in — the only change is
/// its `status`, which now records what superseded it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppliedCorrection {
    pub original: Version,
    pub replacement: Version,
}

pub fn apply_correction(
    original_digest: [u8; 32],
    replacement_digest: [u8; 32],
) -> AppliedCorrection {
    AppliedCorrection {
        original: Version {
            digest: original_digest,
            status: VersionStatus::Superseded {
                by: replacement_digest,
            },
        },
        replacement: Version {
            digest: replacement_digest,
            status: VersionStatus::Original,
        },
    }
}
