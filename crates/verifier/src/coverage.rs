//! Coverage recomputation ("coverage" failure mode), per
//! the protocol specification's step 5: "Recalcular gaps implícitos
//! pelo calendário, membership e períodos; comparar coverageManifest."
//! Reuses `checkpoint::derive_gaps` directly — the same
//! calendar-only derivation already proven there — rather than
//! reimplementing gap logic in this crate.

use checkpoint::{derive_gaps, CalendarPolicy, Gap};

#[derive(Debug, PartialEq, Eq)]
pub struct CoverageMismatch {
    pub recomputed: Vec<Gap>,
    pub claimed: Vec<Gap>,
}

/// Recomputes gaps from `policy`/`last_committed_end_ms`/`now_ms` alone
/// and compares them to what the bundle claims. A bundle that omits a
/// real gap (or invents one that calendar time doesn't support) fails
/// here — coverage is never taken on the bundle's word alone.
pub fn verify_coverage(
    policy: &CalendarPolicy,
    last_committed_end_ms: i64,
    now_ms: i64,
    claimed_gaps: &[Gap],
) -> Result<(), CoverageMismatch> {
    let recomputed = derive_gaps(policy, last_committed_end_ms, now_ms);
    if recomputed != claimed_gaps {
        return Err(CoverageMismatch {
            recomputed,
            claimed: claimed_gaps.to_vec(),
        });
    }
    Ok(())
}
