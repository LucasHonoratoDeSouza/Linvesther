use checkpoint::{CalendarPolicy, Gap};
use verifier::verify_coverage;

const DAY_MS: i64 = 86_400_000;

fn policy() -> CalendarPolicy {
    CalendarPolicy {
        interval_ms: DAY_MS,
        grace_ms: DAY_MS,
    }
}

#[test]
fn claimed_gaps_matching_the_real_calendar_recomputation_verify() {
    let recomputed = checkpoint::derive_gaps(&policy(), 0, 4 * DAY_MS);
    assert!(verify_coverage(&policy(), 0, 4 * DAY_MS, &recomputed).is_ok());
}

#[test]
fn a_bundle_that_omits_a_real_gap_is_rejected() {
    // The calendar says two days were missed; the bundle claims none.
    assert!(verify_coverage(&policy(), 0, 4 * DAY_MS, &[]).is_err());
}

#[test]
fn a_bundle_that_invents_a_gap_the_calendar_does_not_support_is_rejected() {
    let invented = vec![Gap {
        start_ms: 0,
        end_ms: DAY_MS,
    }];
    // No time has actually elapsed past the first interval's deadline.
    assert!(verify_coverage(&policy(), 0, DAY_MS, &invented).is_err());
}
