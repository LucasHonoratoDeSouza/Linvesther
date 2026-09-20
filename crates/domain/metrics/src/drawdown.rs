//! Maximum drawdown, tie-break rule, and recovery time, per
//! the protocol specification: "MDD diário | max_t(1-I_t/max_(u≤t) I_u),
//! magnitude entre 0 e 1; inclui baseline, fechamentos diários e
//! observação terminal obrigatória de perda total... Empate entre
//! episódios de MDD: escolher o pico mais antigo, depois o vale mais
//! antigo."
//!
//! `points` must already include the baseline (`I_0 = 1`), every daily
//! close, and — if a total loss occurred — the mandatory terminal
//! observation at that instant; this module computes over whatever it is
//! given and does not itself assemble that series (that is `returns`
//! the returns crate's / the caller's concern).

use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexPoint {
    pub time_ms: u64,
    pub index: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawdownEpisode {
    pub peak_time_ms: u64,
    pub peak_index: Decimal,
    pub trough_time_ms: u64,
    pub trough_index: Decimal,
    /// `1 - trough_index/peak_index`, in `[0, 1]`.
    pub magnitude: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStatus {
    Recovered { recovered_at_ms: u64 },
    StillOpen,
}

/// Finds the maximum-drawdown episode. Ties are resolved by construction,
/// not a secondary comparison: the running peak only advances on a
/// *strictly greater* index, so the earliest point reaching any given
/// peak level is the one kept; and the magnitude is only replaced on a
/// *strictly greater* drawdown, so the first (chronologically earliest)
/// occurrence of the maximum magnitude — under the earliest peak that
/// produced it — is what's returned. `None` if `points` is empty.
pub fn max_drawdown(points: &[IndexPoint]) -> Option<DrawdownEpisode> {
    let mut running_peak = points.first()?;
    let mut best: Option<DrawdownEpisode> = None;

    for point in points {
        if point.index > running_peak.index {
            running_peak = point;
        }
        // running_peak.index is always > 0 in a well-formed series (an
        // index can be exactly 0 after total loss, but never negative;
        // guard the division defensively anyway).
        if running_peak.index <= Decimal::ZERO {
            continue;
        }
        let magnitude = Decimal::ONE - (point.index / running_peak.index);
        let is_new_max = match &best {
            None => true,
            Some(current) => magnitude > current.magnitude,
        };
        if is_new_max {
            best = Some(DrawdownEpisode {
                peak_time_ms: running_peak.time_ms,
                peak_index: running_peak.index,
                trough_time_ms: point.time_ms,
                trough_index: point.index,
                magnitude,
            });
        }
    }
    best
}

/// Scans forward from `episode.peak_index` for the first point (in
/// `points`, chronological) at or after the peak's time whose index is
/// `>= peak_index`. `None`/`StillOpen` — not an error — if the series
/// never recovers; the caller keeps that as an explicitly open episode.
pub fn recovery_time(points: &[IndexPoint], episode: &DrawdownEpisode) -> RecoveryStatus {
    for point in points {
        if point.time_ms > episode.peak_time_ms && point.index >= episode.peak_index {
            return RecoveryStatus::Recovered {
                recovered_at_ms: point.time_ms,
            };
        }
    }
    RecoveryStatus::StillOpen
}
