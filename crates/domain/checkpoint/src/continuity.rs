//! Continuity segments and late recovery, per the protocol specification:
//! "Recuperação antes do prazo pode completar uma janela... Depois do
//! prazo, dados recuperados são suplementos históricos marcados `late`;
//! não consertam retrospectivamente a continuidade."

use crate::calendar::Gap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Splits `[track_start_ms, track_end_ms)` into continuous segments broken
/// at each gap. `gaps` must be sorted and non-overlapping.
pub fn segments_between(track_start_ms: i64, track_end_ms: i64, gaps: &[Gap]) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut cursor = track_start_ms;
    for gap in gaps {
        if gap.start_ms > cursor {
            segments.push(Segment {
                start_ms: cursor,
                end_ms: gap.start_ms,
            });
        }
        cursor = gap.end_ms.max(cursor);
    }
    if track_end_ms > cursor {
        segments.push(Segment {
            start_ms: cursor,
            end_ms: track_end_ms,
        });
    }
    segments
}

/// Data for `gap` that arrived after `gap.end_ms + policy.grace_ms`. It is
/// never fed back into [`derive_gaps`](crate::calendar::derive_gaps) or
/// [`segments_between`]: the gap it recovers stays recorded, and the
/// segments straddling it stay split. The recovery is a historical
/// supplement only — it cannot retroactively re-join what the calendar
/// already marked discontinuous.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LateSupplement {
    pub gap: Gap,
    pub recovered_at_ms: i64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LateRecoveryError {
    #[error("recovery at {recovered_at_ms} is not late for a gap whose deadline is {deadline_ms}: use the on-time recovery path instead")]
    NotLate {
        recovered_at_ms: i64,
        deadline_ms: i64,
    },
}

/// Records `gap` as recovered-but-late. This never mutates or removes the
/// gap from a prior `derive_gaps` result — callers keep publishing both
/// the gap and this supplement side by side.
pub fn recover_late(
    policy: &crate::calendar::CalendarPolicy,
    gap: Gap,
    recovered_at_ms: i64,
) -> Result<LateSupplement, LateRecoveryError> {
    let deadline_ms = gap.end_ms + policy.grace_ms;
    if recovered_at_ms <= deadline_ms {
        return Err(LateRecoveryError::NotLate {
            recovered_at_ms,
            deadline_ms,
        });
    }
    Ok(LateSupplement {
        gap,
        recovered_at_ms,
    })
}
