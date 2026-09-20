use metrics::{compute_cagr, CagrError};
use rust_decimal::Decimal;
use std::str::FromStr;

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

#[test]
fn cagr_rejects_fewer_than_365_days() {
    let result = compute_cagr(dec("1.0"), dec("1.1"), 364);
    assert_eq!(
        result,
        Err(CagrError::InsufficientSample {
            actual: 364,
            required: 365
        })
    );
}

#[test]
fn cagr_accepts_exactly_365_days() {
    assert!(compute_cagr(dec("1.0"), dec("1.1"), 365).is_ok());
}

#[test]
fn cagr_over_exactly_one_year_equals_the_simple_return() {
    // With d = 365, the exponent 365/d = 1, so CAGR reduces to the simple
    // total return.
    let result = compute_cagr(dec("1.0"), dec("1.21"), 365).unwrap();
    assert_eq!(result.round_dp(8), dec("0.21"));
}

#[test]
fn cagr_rejects_non_positive_start_index() {
    let result = compute_cagr(dec("0"), dec("1.1"), 365);
    assert_eq!(
        result,
        Err(CagrError::NonPositiveIndex {
            index_start: dec("0"),
            index_end: dec("1.1")
        })
    );
}

#[test]
fn cagr_rejects_non_positive_end_index() {
    let result = compute_cagr(dec("1.0"), dec("0"), 365);
    assert!(matches!(result, Err(CagrError::NonPositiveIndex { .. })));
}
