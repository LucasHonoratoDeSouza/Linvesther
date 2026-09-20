use compute_jobs::{
    backoff_delay_ms, decide, jittered_delay_ms, Backoff, DeadLetter, Job, JobState, RetryDecision,
};

const FIVE_SECONDS_MS: i64 = 5_000;
const FIFTEEN_MINUTES_MS: i64 = 15 * 60 * 1_000;

fn policy() -> Backoff {
    Backoff {
        base_ms: FIVE_SECONDS_MS,
        max_ms: FIFTEEN_MINUTES_MS,
        max_attempts: 10,
    }
}

fn job_at_attempt(attempt: u32) -> Job {
    Job {
        id: "job-1".to_string(),
        job_type: "checkpoint".to_string(),
        tenant: "track-1".to_string(),
        dedup_key: "track-1/day-1".to_string(),
        payload_digest: [0u8; 32],
        available_at_ms: 42_000,
        lease_until_ms: None,
        attempt,
        state: JobState::Failed,
        last_error_code: None,
    }
}

#[test]
fn backoff_starts_at_the_base_delay() {
    assert_eq!(backoff_delay_ms(&policy(), 0), FIVE_SECONDS_MS);
}

#[test]
fn backoff_doubles_each_attempt_until_the_cap() {
    let backoff = policy();
    assert_eq!(backoff_delay_ms(&backoff, 1), 10_000);
    assert_eq!(backoff_delay_ms(&backoff, 2), 20_000);
    assert_eq!(backoff_delay_ms(&backoff, 10), FIFTEEN_MINUTES_MS);
}

#[test]
fn backoff_never_exceeds_the_cap() {
    let backoff = policy();
    for attempt in 0..40 {
        assert!(backoff_delay_ms(&backoff, attempt) <= FIFTEEN_MINUTES_MS);
    }
}

#[test]
fn jitter_never_leaves_the_zero_to_delay_range() {
    let delay = 10_000;
    for tenth in 0..=10 {
        let unit = tenth as f64 / 10.0;
        let jittered = jittered_delay_ms(delay, unit);
        assert!(
            (0..=delay).contains(&jittered),
            "jittered delay {jittered} outside [0, {delay}] for unit {unit}"
        );
    }
}

#[test]
fn a_transient_failure_under_the_attempt_limit_retries() {
    let job = job_at_attempt(3);
    let decision = decide(&job, &policy(), "rate_limited", false);
    assert_eq!(
        decision,
        RetryDecision::RetryAfter {
            delay_ms: backoff_delay_ms(&policy(), 3)
        }
    );
}

#[test]
fn reaching_ten_attempts_dead_letters_with_cause_and_deadline_preserved() {
    let job = job_at_attempt(10);
    let decision = decide(&job, &policy(), "still_rate_limited", false);
    assert_eq!(
        decision,
        RetryDecision::DeadLetter(DeadLetter {
            job_id: "job-1".to_string(),
            cause: "still_rate_limited".to_string(),
            deadline_ms: 42_000,
            attempt: 10
        })
    );
}

#[test]
fn a_permanent_error_dead_letters_immediately_without_spending_retry_budget() {
    let job = job_at_attempt(0);
    let decision = decide(&job, &policy(), "permission_denied", true);
    assert_eq!(
        decision,
        RetryDecision::DeadLetter(DeadLetter {
            job_id: "job-1".to_string(),
            cause: "permission_denied".to_string(),
            deadline_ms: 42_000,
            attempt: 0
        })
    );
}
