use compute_jobs::{try_acquire, Job, JobState, LeaseError};

fn pending_job() -> Job {
    Job {
        id: "job-1".to_string(),
        job_type: "checkpoint".to_string(),
        tenant: "track-1".to_string(),
        dedup_key: "track-1/day-1".to_string(),
        payload_digest: [0u8; 32],
        available_at_ms: 0,
        lease_until_ms: None,
        attempt: 0,
        state: JobState::Pending,
        last_error_code: None,
    }
}

#[test]
fn an_unleased_job_can_be_acquired() {
    let job = pending_job();
    let leased = try_acquire(&job, 1_000, 60_000).unwrap();
    assert_eq!(leased.state, JobState::Leased);
    assert_eq!(leased.lease_until_ms, Some(61_000));
}

#[test]
fn a_still_active_lease_blocks_reacquisition() {
    let mut job = pending_job();
    job.lease_until_ms = Some(10_000);
    let result = try_acquire(&job, 5_000, 60_000);
    assert_eq!(
        result,
        Err(LeaseError::StillLeased {
            lease_until_ms: 10_000,
            now_ms: 5_000
        })
    );
}

#[test]
fn an_expired_lease_permits_resumption() {
    let mut job = pending_job();
    job.lease_until_ms = Some(10_000);
    // now_ms is past the old lease's expiry — a worker that crashed and
    // never released it does not block the next attempt.
    let resumed = try_acquire(&job, 10_000, 60_000).unwrap();
    assert_eq!(resumed.state, JobState::Leased);
    assert_eq!(resumed.lease_until_ms, Some(70_000));
}

#[test]
fn lease_expiring_exactly_at_now_is_treated_as_expired() {
    let mut job = pending_job();
    job.lease_until_ms = Some(5_000);
    assert!(try_acquire(&job, 5_000, 1_000).is_ok());
}
