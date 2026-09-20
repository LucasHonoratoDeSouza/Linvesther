//! Exercises the acceptance criteria: MDD 25% no vetor definido (índice
//! 100,120,90,108 → MDD 25%; ainda não recuperado), plus the tie-break
//! rule and recovery time.

use metrics::{max_drawdown, recovery_time, IndexPoint, RecoveryStatus};
use rust_decimal::Decimal;
use std::str::FromStr;

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn point(time_ms: u64, index: &str) -> IndexPoint {
    IndexPoint {
        time_ms,
        index: dec(index),
    }
}

#[test]
fn named_vector_mdd_is_exactly_25_percent_and_unrecovered() {
    let points = [
        point(0, "100"),
        point(1, "120"),
        point(2, "90"),
        point(3, "108"),
    ];
    let episode = max_drawdown(&points).unwrap();

    assert_eq!(episode.magnitude, dec("0.25"));
    assert_eq!(episode.peak_index, dec("120"));
    assert_eq!(episode.peak_time_ms, 1);
    assert_eq!(episode.trough_index, dec("90"));
    assert_eq!(episode.trough_time_ms, 2);

    let recovery = recovery_time(&points, &episode);
    assert_eq!(
        recovery,
        RecoveryStatus::StillOpen,
        "108 < 120: not yet recovered"
    );
}

#[test]
fn magnitude_is_between_zero_and_one() {
    let points = [point(0, "100"), point(1, "50")];
    let episode = max_drawdown(&points).unwrap();
    assert_eq!(episode.magnitude, dec("0.5"));
    assert!(episode.magnitude >= Decimal::ZERO && episode.magnitude <= Decimal::ONE);
}

#[test]
fn monotonically_rising_series_has_zero_drawdown() {
    let points = [point(0, "100"), point(1, "110"), point(2, "120")];
    let episode = max_drawdown(&points).unwrap();
    assert_eq!(episode.magnitude, Decimal::ZERO);
}

#[test]
fn empty_series_has_no_drawdown_episode() {
    assert!(max_drawdown(&[]).is_none());
}

#[test]
fn single_point_has_zero_drawdown() {
    let points = [point(0, "100")];
    let episode = max_drawdown(&points).unwrap();
    assert_eq!(episode.magnitude, Decimal::ZERO);
}

#[test]
fn recovery_is_detected_when_index_returns_to_the_peak() {
    let points = [
        point(0, "100"),
        point(1, "120"),
        point(2, "90"),
        point(3, "120"),
    ];
    let episode = max_drawdown(&points).unwrap();
    let recovery = recovery_time(&points, &episode);
    assert_eq!(recovery, RecoveryStatus::Recovered { recovered_at_ms: 3 });
}

#[test]
fn total_loss_terminal_point_produces_mdd_100_percent() {
    let points = [point(0, "100"), point(1, "0")];
    let episode = max_drawdown(&points).unwrap();
    assert_eq!(episode.magnitude, Decimal::ONE);
}

// --- tie-break rule: earliest peak, then earliest trough ----------------

#[test]
fn tie_break_prefers_the_earliest_peak() {
    // Two equal-depth 20% drawdowns from two different peaks (100 and
    // then a later, re-attained 100): the earliest peak must be chosen.
    let points = [
        point(0, "100"), // first peak
        point(1, "80"),  // 20% drawdown from peak @0
        point(2, "100"), // back to 100, but NOT a new peak (not strictly greater)
        point(3, "80"),  // another 20% drawdown, same magnitude
    ];
    let episode = max_drawdown(&points).unwrap();
    assert_eq!(
        episode.peak_time_ms, 0,
        "the earliest point reaching the peak level must be kept, not the later re-attainment"
    );
    assert_eq!(
        episode.trough_time_ms, 1,
        "first occurrence of the max magnitude is kept, not a later equal one"
    );
}

#[test]
fn tie_break_prefers_the_earliest_trough_under_the_same_peak() {
    // Same peak, magnitude dips to exactly the same worst level twice.
    let points = [
        point(0, "100"),
        point(1, "75"),
        point(2, "80"),
        point(3, "75"),
    ];
    let episode = max_drawdown(&points).unwrap();
    assert_eq!(
        episode.trough_time_ms, 1,
        "the first trough at the worst magnitude must be kept"
    );
}
