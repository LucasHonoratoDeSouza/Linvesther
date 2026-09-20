//! Transactional job queue and outbox: idempotent effect
//! application so a crash between effects is resumed without
//! duplication, lease acquisition where an expired lease always permits
//! resumption, and retry/dead-letter decisions that preserve the failure
//! cause and the job's original deadline.

pub mod job;
pub mod lease;
pub mod outbox;
pub mod retry;

pub use job::{Job, JobState};
pub use lease::{try_acquire, LeaseError};
pub use outbox::{Effect, Outbox};
pub use retry::{backoff_delay_ms, decide, jittered_delay_ms, Backoff, DeadLetter, RetryDecision};
