//! Native (host-target) tests of the guest's pure logic — no proving,
//! same code path `src/main.rs` runs inside the zkVM. Sharpe/Sortino/CAGR obey sample/precision requirements and
//! reject a threshold claim that crosses the certified error interval.

use guest::stats::{
    cagr_bracket, sharpe_bracket, sortino_bracket, ClaimOperator, ClaimVerdict, StatsError,
};
use guest::{run, GuestInput, MetricClaim};

const SCALE: i128 = 1_000_000_000_000;

fn alternating_returns(n: usize) -> Vec<i128> {
    (0..n)
        .map(|i| {
            if i % 2 == 0 {
                SCALE / 100
            } else {
                -SCALE / 200
            }
        }) // +1%, -0.5%
        .collect()
}

#[test]
fn sharpe_rejects_a_sample_smaller_than_thirty_days() {
    let err = sharpe_bracket(&alternating_returns(29), 0).unwrap_err();
    assert_eq!(
        err,
        StatsError::InsufficientSample {
            actual: 29,
            required: 30
        }
    );
}

#[test]
fn sharpe_rejects_zero_variance_instead_of_publishing_infinity() {
    let constant = vec![SCALE / 100; 30];
    assert_eq!(
        sharpe_bracket(&constant, 0).unwrap_err(),
        StatsError::ZeroVariance
    );
}

#[test]
fn sharpe_approves_a_greater_than_claim_clearly_below_the_bracket() {
    let bracket = sharpe_bracket(&alternating_returns(60), 0).unwrap();
    assert!(
        bracket.lower > 0,
        "this fixture's mean excess return is positive"
    );
    let verdict =
        guest::stats::evaluate_claim(bracket, ClaimOperator::GreaterThan(bracket.lower - SCALE));
    assert_eq!(verdict, ClaimVerdict::Approved);
}

#[test]
fn sharpe_rejects_a_greater_than_claim_clearly_above_the_bracket() {
    let bracket = sharpe_bracket(&alternating_returns(60), 0).unwrap();
    let verdict =
        guest::stats::evaluate_claim(bracket, ClaimOperator::GreaterThan(bracket.upper + SCALE));
    assert_eq!(verdict, ClaimVerdict::Rejected);
}

#[test]
fn sharpe_claim_straddling_the_bracket_is_indeterminate_precision() {
    let bracket = sharpe_bracket(&alternating_returns(60), 0).unwrap();
    let midpoint = bracket.lower + (bracket.upper - bracket.lower) / 2;
    let verdict = guest::stats::evaluate_claim(bracket, ClaimOperator::GreaterThan(midpoint));
    assert_eq!(verdict, ClaimVerdict::IndeterminatePrecision);
}

#[test]
fn sortino_rejects_a_sample_with_no_downside() {
    let all_positive = vec![SCALE / 100; 30];
    assert_eq!(
        sortino_bracket(&all_positive, 0).unwrap_err(),
        StatsError::NoDownside
    );
}

#[test]
fn sortino_approves_a_greater_than_claim_clearly_below_the_bracket() {
    let bracket = sortino_bracket(&alternating_returns(60), 0).unwrap();
    let verdict =
        guest::stats::evaluate_claim(bracket, ClaimOperator::GreaterThan(bracket.lower - SCALE));
    assert_eq!(verdict, ClaimVerdict::Approved);
}

#[test]
fn cagr_rejects_fewer_than_365_days() {
    let err = cagr_bracket(SCALE, SCALE * 2, 364).unwrap_err();
    assert_eq!(
        err,
        StatsError::InsufficientSample {
            actual: 364,
            required: 365
        }
    );
}

#[test]
fn cagr_rejects_a_non_positive_index() {
    assert_eq!(
        cagr_bracket(0, SCALE, 365).unwrap_err(),
        StatsError::NonPositiveIndex
    );
    assert_eq!(
        cagr_bracket(SCALE, -SCALE, 365).unwrap_err(),
        StatsError::NonPositiveIndex
    );
}

#[test]
fn cagr_over_exactly_one_year_equals_the_total_return_within_a_tiny_margin() {
    // days_elapsed == 365 means exponent 365/365 == 1, so CAGR should
    // equal the plain total return (doubling -> ~100%).
    let bracket = cagr_bracket(SCALE, SCALE * 2, 365).unwrap();
    let one = SCALE; // 100%
    assert!(
        bracket.lower <= one && one <= bracket.upper,
        "bracket [{}, {}] should contain 100%",
        bracket.lower,
        bracket.upper
    );
    let width = bracket.upper - bracket.lower;
    assert!(
        width < SCALE / 1_000_000_000,
        "bracket should be far tighter than a millionth of a percent, got width {width}"
    );
}

#[test]
fn cagr_approves_a_low_threshold_and_rejects_a_high_one_for_a_doubling_over_one_year() {
    let bracket = cagr_bracket(SCALE, SCALE * 2, 365).unwrap();
    let low = guest::stats::evaluate_claim(bracket, ClaimOperator::GreaterThan(SCALE / 2)); // > 50%
    assert_eq!(low, ClaimVerdict::Approved);
    let high = guest::stats::evaluate_claim(bracket, ClaimOperator::GreaterThan(SCALE * 10)); // > 1000%
    assert_eq!(high, ClaimVerdict::Rejected);
}

#[test]
fn cagr_over_a_decade_stays_within_representable_bounds_for_extreme_growth() {
    // A 1000x total return is astronomically large raised to 365, which
    // is exactly why this module avoids that reformulation — confirm it
    // still resolves cleanly over a ten-year (3650-day) horizon.
    let bracket = cagr_bracket(SCALE, SCALE * 1000, 3650).unwrap();
    assert!(
        bracket.lower > 0,
        "1000x growth over a decade is strongly positive CAGR"
    );
}

#[test]
fn run_dispatches_a_sharpe_claim_and_reports_its_verdict() {
    let input = GuestInput {
        claim: MetricClaim::Sharpe {
            returns: alternating_returns(60),
            rf_daily: 0,
            operator: ClaimOperator::GreaterThan(-SCALE),
        },
    };
    let output = run(&input).unwrap();
    assert_eq!(output.verdict, ClaimVerdict::Approved);
}

#[test]
fn run_rejects_an_insufficient_cagr_sample_without_proving() {
    let input = GuestInput {
        claim: MetricClaim::Cagr {
            index_start: SCALE,
            index_end: SCALE * 2,
            days_elapsed: 100,
            operator: ClaimOperator::GreaterThan(0),
        },
    };
    assert!(run(&input).is_err());
}
