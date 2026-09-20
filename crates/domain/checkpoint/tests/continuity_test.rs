use checkpoint::{
    derive_gaps, recover_late, segments_between, CalendarPolicy, Gap, LateRecoveryError, Segment,
};

const DAY_MS: i64 = 86_400_000;

fn daily_policy() -> CalendarPolicy {
    CalendarPolicy {
        interval_ms: DAY_MS,
        grace_ms: DAY_MS,
    }
}

#[test]
fn a_gap_splits_the_track_into_two_segments() {
    let gaps = vec![Gap {
        start_ms: DAY_MS,
        end_ms: 2 * DAY_MS,
    }];
    let segments = segments_between(0, 4 * DAY_MS, &gaps);
    assert_eq!(
        segments,
        vec![
            Segment {
                start_ms: 0,
                end_ms: DAY_MS
            },
            Segment {
                start_ms: 2 * DAY_MS,
                end_ms: 4 * DAY_MS
            }
        ]
    );
}

#[test]
fn late_recovery_does_not_restore_continuity() {
    let policy = daily_policy();
    // Day 1 (0..DAY_MS) is missed and only recovered well after its
    // deadline (end+grace = 2*DAY_MS).
    let gaps = derive_gaps(&policy, 0, 4 * DAY_MS);
    let gap = gaps[0];
    let segments_before = segments_between(0, 4 * DAY_MS, &gaps);

    let supplement = recover_late(&policy, gap, 3 * DAY_MS).unwrap();
    assert_eq!(supplement.gap, gap);

    // The gap list used to derive segments is untouched by the recovery —
    // there is no function that takes a LateSupplement and edits it out.
    let segments_after = segments_between(0, 4 * DAY_MS, &gaps);
    assert_eq!(
        segments_before, segments_after,
        "a late supplement must never re-join segments the calendar already split"
    );
}

#[test]
fn recovery_before_the_deadline_is_rejected_as_not_late() {
    let policy = daily_policy();
    let gap = Gap {
        start_ms: 0,
        end_ms: DAY_MS,
    };
    let deadline = gap.end_ms + policy.grace_ms;
    let result = recover_late(&policy, gap, deadline);
    assert_eq!(
        result,
        Err(LateRecoveryError::NotLate {
            recovered_at_ms: deadline,
            deadline_ms: deadline
        })
    );
}

#[test]
fn no_gaps_yields_one_continuous_segment() {
    let segments = segments_between(0, 3 * DAY_MS, &[]);
    assert_eq!(
        segments,
        vec![Segment {
            start_ms: 0,
            end_ms: 3 * DAY_MS
        }]
    );
}
