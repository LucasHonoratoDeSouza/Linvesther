//! Backoff and dead-letter, per the operations design: "Backoff exponencial com
//! jitter: 5 s até 15 min... Após 10 tentativas, dead-letter e alerta;
//! calendário de gap continua independente. Erros permanentes de
//! permissão/schema/UID não entram em loop cego."

use crate::job::Job;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    pub base_ms: i64,
    pub max_ms: i64,
    pub max_attempts: u32,
}

/// Exponential delay for `attempt`, capped at `backoff.max_ms`. Monotonic
/// non-decreasing in `attempt` by construction (each step doubles, then
/// saturates at the cap), so it never has to be re-derived per call site.
pub fn backoff_delay_ms(backoff: &Backoff, attempt: u32) -> i64 {
    let shift = attempt.min(62);
    let exponential = backoff.base_ms.saturating_mul(1i64 << shift);
    exponential.min(backoff.max_ms)
}

/// Applies jitter to `delay_ms` without ever leaving `[0, delay_ms]`.
/// `random_unit` is injected by the caller (rather than sampled inside
/// this function) so the bound holds for every value a caller could pass
/// in, not just whatever a hidden RNG happens to draw during a test.
pub fn jittered_delay_ms(delay_ms: i64, random_unit: f64) -> i64 {
    let clamped_unit = random_unit.clamp(0.0, 1.0);
    (delay_ms as f64 * clamped_unit).round() as i64
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadLetter {
    pub job_id: String,
    pub cause: String,
    /// The job's original `available_at_ms`, preserved unchanged — a
    /// dead-lettered job still carries when it was supposed to run.
    pub deadline_ms: i64,
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryDecision {
    RetryAfter { delay_ms: i64 },
    DeadLetter(DeadLetter),
}

/// `is_permanent` covers errors like permission/schema/UID mismatches
/// that the operations design says must never "entrar em loop cego" — they go
/// straight to dead-letter on the first occurrence, without spending any
/// of the retry budget.
pub fn decide(job: &Job, backoff: &Backoff, error_code: &str, is_permanent: bool) -> RetryDecision {
    if is_permanent || job.attempt >= backoff.max_attempts {
        return RetryDecision::DeadLetter(DeadLetter {
            job_id: job.id.clone(),
            cause: error_code.to_string(),
            deadline_ms: job.available_at_ms,
            attempt: job.attempt,
        });
    }
    RetryDecision::RetryAfter {
        delay_ms: backoff_delay_ms(backoff, job.attempt),
    }
}
