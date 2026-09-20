//! Pure guest logic for the performance claims guest: validating A0
//! origin, reconciling inputs and computing return/MDD/capital. This
//! module has no dependency on `risc0_zkvm::guest::env`, so it compiles
//! and is directly unit-testable for the host (native) target, in
//! addition to being the exact code `src/main.rs` runs inside the zkVM —
//! one implementation, so a test exercising it never drifts from what
//! the guest actually proves.

pub mod compute;
pub mod envelope;
pub mod origin;

use compute::ComputeError;
use envelope::SourceEnvelope;
use origin::OriginError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestInput {
    pub envelope: SourceEnvelope,
    pub verifying_key_sec1: Vec<u8>,
    /// 64-byte compact `r || s` ECDSA signature.
    pub signature_compact: Vec<u8>,
    /// Fixed-point NAV samples (scaled by [`compute::SCALE`]), one per
    /// covered checkpoint day, chronological, first-to-last spanning the
    /// envelope's `period_start_ms..period_end_ms`.
    pub nav: Vec<i64>,
    /// Ledger deltas, same scale, expected to sum to `nav`'s net change.
    pub ledger_deltas: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestOutput {
    pub envelope_digest: [u8; 32],
    /// Which collector key signed the source data. Committed so a verifier
    /// can tell a trusted collector from someone who signed their own
    /// figures: the proof shows the calculation is right, this shows whose
    /// data it was calculated from.
    pub signer_fingerprint: [u8; 32],
    pub period_start_ms: i64,
    pub period_end_ms: i64,
    pub twr_index_scaled: i128,
    pub mdd_bp: i64,
    pub capital: i64,
}

#[derive(Debug)]
pub enum GuestError {
    Origin(OriginError),
    Compute(ComputeError),
}

impl From<OriginError> for GuestError {
    fn from(value: OriginError) -> Self {
        GuestError::Origin(value)
    }
}

impl From<ComputeError> for GuestError {
    fn from(value: ComputeError) -> Self {
        GuestError::Compute(value)
    }
}

/// Validates A0 origin, reconciles the ledger against the NAV series, and
/// computes the TWR index, MDD and final capital — fixing every one of
/// those into the returned [`GuestOutput`]. Any failure (bad signature, a
/// tampered field, an unreconciled ledger) is `Err`, and the guest never
/// commits a journal for a rejected input.
pub fn run(input: &GuestInput) -> Result<GuestOutput, GuestError> {
    let origin = origin::verify_origin(
        &input.envelope,
        &input.verifying_key_sec1,
        &input.signature_compact,
    )?;

    compute::reconcile(&input.nav, &input.ledger_deltas)?;
    let twr_index_scaled = compute::twr_index(&input.nav)?;
    let mdd_bp = compute::max_drawdown_bp(&input.nav)?;
    let capital = *input.nav.last().ok_or(ComputeError::EmptyNav)?;

    Ok(GuestOutput {
        envelope_digest: origin.envelope_digest,
        signer_fingerprint: origin.signer_fingerprint,
        period_start_ms: input.envelope.period_start_ms,
        period_end_ms: input.envelope.period_end_ms,
        twr_index_scaled,
        mdd_bp,
        capital,
    })
}
