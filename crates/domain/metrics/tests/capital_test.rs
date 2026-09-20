use metrics::{consistency, current_capital, sustained_capital};
use rust_decimal::Decimal;
use std::str::FromStr;

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

#[test]
fn current_capital_reports_nav_and_age() {
    let capital = current_capital(dec("1000"), 5_000, 8_000);
    assert_eq!(capital.nav, dec("1000"));
    assert_eq!(capital.as_of_ms, 5_000);
    assert_eq!(capital.age_ms, 3_000);
}

#[test]
fn sustained_capital_includes_the_baseline_not_just_daily_closes() {
    // Baseline is the true minimum; daily closes are all higher.
    let result = sustained_capital(dec("50"), &[dec("100"), dec("90"), dec("80")]);
    assert_eq!(result, dec("50"));
}

#[test]
fn sustained_capital_uses_the_lowest_daily_close_when_below_baseline() {
    let result = sustained_capital(dec("100"), &[dec("90"), dec("70"), dec("95")]);
    assert_eq!(result, dec("70"));
}

#[test]
fn sustained_capital_with_no_daily_closes_is_just_the_baseline() {
    let result = sustained_capital(dec("100"), &[]);
    assert_eq!(result, dec("100"));
}

#[test]
fn consistency_counts_only_strictly_positive_periods() {
    let returns = vec![dec("0.01"), dec("0"), dec("-0.01"), dec("0.02")];
    let c = consistency(&returns);
    assert_eq!(c.positive_periods, 2, "zero is not positive");
    assert_eq!(c.total_periods, 4);
    assert_eq!(c.ratio().unwrap(), dec("0.5"));
}

#[test]
fn consistency_with_no_periods_has_no_ratio() {
    let c = consistency(&[]);
    assert_eq!(
        c.ratio(),
        None,
        "zero denominator must not be silently treated as zero or one"
    );
}
