//! Local agent: local collection/proving where credentials and
//! witness data never leave the machine, results are deterministic
//! given the same input, and a restart resumes without skipping or
//! duplicating events.

pub mod continuity;
pub mod execution;

pub use continuity::{events_to_process, LocalAgentState};
pub use execution::{
    canonical_result_bytes, canonical_result_digest, execute_locally, LedgerEvent, LocalOnlyInput,
    OperatorSafeResult,
};
