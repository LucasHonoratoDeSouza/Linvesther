//! Job record, per the operations design:
//! "Jobs possuem `id`, tipo, tenant, dedupKey, payloadDigest, availableAt,
//! leaseUntil, attempt, state e lastErrorCode."

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Pending,
    Leased,
    Succeeded,
    Failed,
    DeadLetter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub job_type: String,
    pub tenant: String,
    pub dedup_key: String,
    pub payload_digest: [u8; 32],
    pub available_at_ms: i64,
    pub lease_until_ms: Option<i64>,
    pub attempt: u32,
    pub state: JobState,
    pub last_error_code: Option<String>,
}
