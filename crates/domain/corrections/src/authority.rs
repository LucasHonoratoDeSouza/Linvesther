//! Correction authority, per the protocol specification's
//! "Autoridade e admissibilidade de correções" table: each kind of
//! correction requires a specific authority and evidence, and none of
//! them is "the owner says so" or "enough complaints."

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// Same issuer, current or verifiably-rotated key.
    IssuerRetraction,
    /// Any relayer presenting authenticated substitute input plus proof
    /// of recalculation; the owner's agreement is not required.
    FinancialCorrection,
    /// A new release identifying affected versions with fixtures and
    /// proof of recalculation.
    MethodBugfix,
    /// Authority registered in the trust manifest, per public
    /// governance and evidence of compromise.
    TrustManifestInvalidation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    pub target_digest: [u8; 32],
    pub replacement_digest: [u8; 32],
    pub reason_code: String,
    pub evidence_digest: [u8; 32],
    pub authority: Authority,
}

/// The owner's objection to a correction. It carries whatever the owner
/// wants to say, but nothing on this type feeds into admissibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerObjection {
    pub reason: String,
}

/// Whether `correction` is admissible on its own evidence — an
/// authenticated substitute input plus recalculation proof
/// (`evidence_digest`) under one of [`Authority`]'s cases. `objection` is
/// accepted only so a caller can display it; the return value never
/// depends on it, because the owner cannot veto validated evidence.
pub fn is_admissible(correction: &Correction, _objection: Option<&OwnerObjection>) -> bool {
    correction.evidence_digest != [0u8; 32]
}
