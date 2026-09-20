//! Lease acquisition, per the operations design: "PostgreSQL `FOR UPDATE SKIP
//! LOCKED` distribui trabalho... Lease expirada permite retomada."

use crate::job::{Job, JobState};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LeaseError {
    #[error("job is already leased until {lease_until_ms}, which has not expired at {now_ms}")]
    StillLeased { lease_until_ms: i64, now_ms: i64 },
}

/// Leases `job` for `worker` until `now_ms + lease_duration_ms`. Succeeds
/// whenever there is no lease, or the existing lease's `lease_until_ms`
/// is at or before `now_ms` — an expired lease never blocks resumption,
/// no matter which worker held it before.
pub fn try_acquire(job: &Job, now_ms: i64, lease_duration_ms: i64) -> Result<Job, LeaseError> {
    if let Some(lease_until_ms) = job.lease_until_ms {
        if lease_until_ms > now_ms {
            return Err(LeaseError::StillLeased {
                lease_until_ms,
                now_ms,
            });
        }
    }
    let mut leased = job.clone();
    leased.state = JobState::Leased;
    leased.lease_until_ms = Some(now_ms + lease_duration_ms);
    Ok(leased)
}
