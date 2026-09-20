use guest::compute::{max_drawdown_bp, reconcile, twr_index, ComputeError, SCALE};

fn nav(values: &[i64]) -> Vec<i64> {
    values.iter().map(|v| v * SCALE).collect()
}

#[test]
fn reconciliation_holds_when_ledger_deltas_sum_to_the_nav_change() {
    let nav = nav(&[100, 110]);
    let deltas = vec![10 * SCALE];
    assert!(reconcile(&nav, &deltas).is_ok());
}

#[test]
fn reconciliation_rejects_a_mismatched_ledger() {
    let nav = nav(&[100, 110]);
    let deltas = vec![5 * SCALE];
    assert_eq!(
        reconcile(&nav, &deltas),
        Err(ComputeError::ReconciliationMismatch {
            expected: 10 * SCALE,
            got: 5 * SCALE
        })
    );
}

#[test]
fn twr_index_of_a_flat_series_stays_at_one() {
    let nav = nav(&[100, 100, 100]);
    assert_eq!(twr_index(&nav).unwrap(), SCALE as i128);
}

#[test]
fn twr_index_compounds_a_ten_percent_gain_then_a_ten_percent_loss() {
    // 100 -> 110 (+10%) -> 99 (-10%): index ends at 0.99.
    let nav = nav(&[100, 110, 99]);
    let index = twr_index(&nav).unwrap();
    assert_eq!(index, 990_000);
}

#[test]
fn max_drawdown_from_the_named_metrics_vector() {
    // 100 -> 120 -> 90 -> 108: peak 120, trough 90, MDD = 25% = 2500 bp.
    let nav = nav(&[100, 120, 90, 108]);
    assert_eq!(max_drawdown_bp(&nav).unwrap(), 2_500);
}

#[test]
fn a_monotonically_rising_series_has_zero_drawdown() {
    let nav = nav(&[100, 110, 120, 130]);
    assert_eq!(max_drawdown_bp(&nav).unwrap(), 0);
}

#[test]
fn empty_nav_is_rejected_everywhere() {
    assert_eq!(reconcile(&[], &[]), Err(ComputeError::EmptyNav));
    assert_eq!(twr_index(&[]), Err(ComputeError::EmptyNav));
    assert_eq!(max_drawdown_bp(&[]), Err(ComputeError::EmptyNav));
}

#[test]
fn non_positive_nav_is_rejected_rather_than_dividing_by_zero_or_negative() {
    let nav = vec![100 * SCALE, 0, 50 * SCALE];
    assert!(matches!(
        twr_index(&nav),
        Err(ComputeError::NonPositiveNav { .. })
    ));
    assert!(matches!(
        max_drawdown_bp(&nav),
        Err(ComputeError::NonPositiveNav { .. })
    ));
}
