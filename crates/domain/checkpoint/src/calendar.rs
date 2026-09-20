//! Calendar-derived gaps, per the protocol specification's "Continuidade"
//! section: "Ausência de checkpoint é verificável pelo último período e
//! pelo calendário; qualquer verificador deriva o gap sem confiar na UI
//! ou em evento voluntário do usuário."

/// A fixed cadence (e.g. one UTC day) plus the grace period the source is
/// allowed before its checkpoint is considered missed (e.g. `end+24h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarPolicy {
    pub interval_ms: i64,
    pub grace_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gap {
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Derives every interval missed since `last_committed_end_ms`, using only
/// `policy` and the current time `now_ms`. There is no parameter for a
/// user-supplied or "voluntary" event: a verifier holding nothing but the
/// calendar and the last committed checkpoint's end can compute the exact
/// same gaps this function returns.
pub fn derive_gaps(policy: &CalendarPolicy, last_committed_end_ms: i64, now_ms: i64) -> Vec<Gap> {
    let mut gaps = Vec::new();
    let mut start = last_committed_end_ms;
    loop {
        let end = start + policy.interval_ms;
        let deadline = end + policy.grace_ms;
        if deadline > now_ms {
            break;
        }
        gaps.push(Gap {
            start_ms: start,
            end_ms: end,
        });
        start = end;
    }
    gaps
}
