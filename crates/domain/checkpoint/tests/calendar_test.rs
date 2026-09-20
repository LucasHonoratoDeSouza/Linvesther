use checkpoint::{derive_gaps, CalendarPolicy};

const DAY_MS: i64 = 86_400_000;

fn daily_policy() -> CalendarPolicy {
    CalendarPolicy {
        interval_ms: DAY_MS,
        grace_ms: DAY_MS,
    }
}

#[test]
fn no_gap_before_the_deadline_of_the_next_interval() {
    // Last checkpoint ended at t=0; we are still inside its interval
    // (before end+grace), so nothing is missed yet.
    let gaps = derive_gaps(&daily_policy(), 0, DAY_MS);
    assert!(gaps.is_empty());
}

#[test]
fn exactly_at_the_deadline_the_interval_is_missed() {
    let policy = daily_policy();
    let gaps = derive_gaps(&policy, 0, 2 * DAY_MS);
    assert_eq!(
        gaps,
        vec![checkpoint::Gap {
            start_ms: 0,
            end_ms: DAY_MS
        }]
    );
}

#[test]
fn multiple_missed_intervals_are_all_derived() {
    let policy = daily_policy();
    let gaps = derive_gaps(&policy, 0, 4 * DAY_MS);
    assert_eq!(
        gaps,
        vec![
            checkpoint::Gap {
                start_ms: 0,
                end_ms: DAY_MS
            },
            checkpoint::Gap {
                start_ms: DAY_MS,
                end_ms: 2 * DAY_MS
            },
            checkpoint::Gap {
                start_ms: 2 * DAY_MS,
                end_ms: 3 * DAY_MS
            },
        ]
    );
}

#[test]
fn up_to_date_track_has_no_gaps() {
    let policy = daily_policy();
    let gaps = derive_gaps(&policy, 10 * DAY_MS, 10 * DAY_MS);
    assert!(gaps.is_empty());
}

// derive_gaps's signature takes only a policy, the last committed end and
// "now" — there is no parameter through which a user could report or
// suppress a gap. This is enforced by the type system, not by a runtime
// check: the call above compiles with no such argument to pass.
