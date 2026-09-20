//! Restart continuity: the local agent's own progress cursor is
//! the only state a restart needs — reprocessing from it never skips or
//! duplicates an event, the same invariant `services/compute/jobs`
//! established for the hosted job queue, applied here to a single local
//! process that can be killed and restarted at any point.

use crate::execution::LedgerEvent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalAgentState {
    /// The index, in the full event stream, of the last event this
    /// agent has successfully processed. `0` means nothing processed
    /// yet.
    pub processed_through: usize,
}

impl LocalAgentState {
    pub const INITIAL: LocalAgentState = LocalAgentState {
        processed_through: 0,
    };
}

/// Given the *full* event stream (as if freshly re-read from disk after
/// a restart) and the last saved state, returns the events still to be
/// processed and the state to save after processing them — never
/// re-includes an event at or before `processed_through`, and never
/// skips one after it.
pub fn events_to_process(
    state: LocalAgentState,
    all_events: &[LedgerEvent],
) -> (&[LedgerEvent], LocalAgentState) {
    let remaining = &all_events[state.processed_through.min(all_events.len())..];
    let next_state = LocalAgentState {
        processed_through: all_events.len(),
    };
    (remaining, next_state)
}
