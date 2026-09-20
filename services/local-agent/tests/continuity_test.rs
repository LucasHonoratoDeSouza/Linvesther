use local_agent::{events_to_process, LedgerEvent, LocalAgentState};

fn events(n: usize) -> Vec<LedgerEvent> {
    (0..n)
        .map(|i| LedgerEvent {
            id: format!("evt-{i}"),
            delta: i as i64,
        })
        .collect()
}

#[test]
fn a_fresh_agent_processes_every_event_from_the_start() {
    let all = events(5);
    let (remaining, next_state) = events_to_process(LocalAgentState::INITIAL, &all);
    assert_eq!(remaining.len(), 5);
    assert_eq!(next_state.processed_through, 5);
}

#[test]
fn restarting_after_partial_progress_resumes_without_skipping_or_duplicating() {
    let all = events(10);

    // First run: process the first 6, "crash", save state.
    let (first_batch, state_after_first_run) =
        events_to_process(LocalAgentState::INITIAL, &all[..6]);
    assert_eq!(first_batch.len(), 6);
    assert_eq!(state_after_first_run.processed_through, 6);

    // Restart: the full event stream is re-read from disk, but the
    // saved state means only the truly-new events are processed.
    let (remaining, state_after_restart) = events_to_process(state_after_first_run, &all);
    let remaining_ids: Vec<&str> = remaining.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(remaining_ids, vec!["evt-6", "evt-7", "evt-8", "evt-9"]);
    assert_eq!(state_after_restart.processed_through, 10);
}

#[test]
fn processing_nothing_new_after_a_restart_is_a_no_op() {
    let all = events(5);
    let state = LocalAgentState {
        processed_through: 5,
    };
    let (remaining, next_state) = events_to_process(state, &all);
    assert!(remaining.is_empty());
    assert_eq!(next_state.processed_through, 5);
}
