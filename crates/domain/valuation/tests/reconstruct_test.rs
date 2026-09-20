use rust_decimal::Decimal;
use std::str::FromStr;
use valuation::{reconstruct_quantity, BalanceSnapshot, ReconstructError, TimedDelta};

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

#[test]
fn snapshot_at_the_cut_needs_no_adjustment() {
    let snapshot = BalanceSnapshot {
        free: dec("10"),
        locked: dec("5"),
        observed_at_ms: 1_000,
    };
    let qty = reconstruct_quantity(&snapshot, &[], 1_000).unwrap();
    assert_eq!(qty, dec("15"), "free + locked, counted once");
}

#[test]
fn free_and_locked_are_combined_exactly_once() {
    let snapshot = BalanceSnapshot {
        free: dec("10"),
        locked: dec("5"),
        observed_at_ms: 1_000,
    };
    assert_eq!(snapshot.total(), dec("15"));
}

#[test]
fn forward_reconstruction_applies_events_after_snapshot_up_to_cut() {
    let snapshot = BalanceSnapshot {
        free: dec("100"),
        locked: dec("0"),
        observed_at_ms: 1_000,
    };
    let events = [
        TimedDelta {
            time_ms: 1_500,
            delta: dec("10"),
        },
        TimedDelta {
            time_ms: 2_500,
            delta: dec("5"),
        },
    ];
    // Cut at 2_000: only the 1_500 event applies, not the 2_500 one.
    let qty = reconstruct_quantity(&snapshot, &events, 2_000).unwrap();
    assert_eq!(qty, dec("110"));
}

#[test]
fn backward_reconstruction_undoes_events_between_cut_and_snapshot() {
    let snapshot = BalanceSnapshot {
        free: dec("110"),
        locked: dec("0"),
        observed_at_ms: 2_000,
    };
    let events = [TimedDelta {
        time_ms: 1_500,
        delta: dec("10"),
    }];
    // Cut is before the snapshot: the +10 that happened between cut and
    // snapshot must be undone to get back to the earlier true value.
    let qty = reconstruct_quantity(&snapshot, &events, 1_000).unwrap();
    assert_eq!(qty, dec("100"));
}

#[test]
fn events_outside_the_relevant_window_are_ignored() {
    let snapshot = BalanceSnapshot {
        free: dec("100"),
        locked: dec("0"),
        observed_at_ms: 1_000,
    };
    let events = [TimedDelta {
        time_ms: 500,
        delta: dec("999"),
    }]; // before snapshot, cut is forward
    let qty = reconstruct_quantity(&snapshot, &events, 2_000).unwrap();
    assert_eq!(
        qty,
        dec("100"),
        "an event before the snapshot must not be re-applied going forward"
    );
}

#[test]
fn event_exactly_at_snapshot_time_is_ambiguous() {
    let snapshot = BalanceSnapshot {
        free: dec("100"),
        locked: dec("0"),
        observed_at_ms: 1_000,
    };
    let events = [TimedDelta {
        time_ms: 1_000,
        delta: dec("10"),
    }];
    let result = reconstruct_quantity(&snapshot, &events, 2_000);
    assert_eq!(
        result,
        Err(ReconstructError::AmbiguousOrdering {
            event_time_ms: 1_000,
            snapshot_time_ms: 1_000
        })
    );
}

#[test]
fn ambiguity_is_detected_even_if_the_event_is_outside_the_cut_window() {
    // The tie itself is the problem, regardless of which direction the
    // cut is in relative to it.
    let snapshot = BalanceSnapshot {
        free: dec("100"),
        locked: dec("0"),
        observed_at_ms: 1_000,
    };
    let events = [TimedDelta {
        time_ms: 1_000,
        delta: dec("10"),
    }];
    let result = reconstruct_quantity(&snapshot, &events, 500);
    assert!(result.is_err());
}
