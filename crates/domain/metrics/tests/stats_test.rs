//! Exercises the acceptance criteria: amostras mínimas, rf e precisão obedecem
//! spec (Sharpe/Sortino/volatilidade exigem >= 30 dias; rf é parâmetro
//! explícito; sem variância -> ZERO_VARIANCE, não infinito; sem downside
//! -> NO_DOWNSIDE).

use metrics::{annualized_volatility, mean, sample_variance, sharpe, sortino, StatsError};
use rust_decimal::Decimal;
use std::str::FromStr;

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn repeat(value: &str, n: usize) -> Vec<Decimal> {
    vec![dec(value); n]
}

#[test]
fn mean_of_empty_series_is_an_error_not_zero() {
    let result = mean(&[]);
    assert_eq!(
        result,
        Err(StatsError::InsufficientSample {
            actual: 0,
            required: 1
        })
    );
}

#[test]
fn mean_matches_manual_computation() {
    let returns = vec![dec("0.01"), dec("0.02"), dec("0.03")];
    assert_eq!(mean(&returns).unwrap(), dec("0.02"));
}

#[test]
fn sample_variance_requires_at_least_two_points() {
    let result = sample_variance(&[dec("0.01")]);
    assert_eq!(
        result,
        Err(StatsError::InsufficientSample {
            actual: 1,
            required: 2
        })
    );
}

#[test]
fn sample_variance_of_constant_series_is_zero() {
    let returns = repeat("0.01", 10);
    assert_eq!(sample_variance(&returns).unwrap(), Decimal::ZERO);
}

// --- minimum sample: 30 days for volatility/Sharpe/Sortino --------------

#[test]
fn volatility_rejects_fewer_than_30_days() {
    let returns = repeat("0.01", 29);
    let result = annualized_volatility(&returns);
    assert_eq!(
        result,
        Err(StatsError::InsufficientSample {
            actual: 29,
            required: 30
        })
    );
}

#[test]
fn volatility_accepts_exactly_30_days() {
    let mut returns = repeat("0.01", 29);
    returns.push(dec("0.02"));
    assert!(annualized_volatility(&returns).is_ok());
}

#[test]
fn sharpe_rejects_fewer_than_30_days() {
    let returns = repeat("0.01", 29);
    let result = sharpe(&returns, Decimal::ZERO);
    assert_eq!(
        result,
        Err(StatsError::InsufficientSample {
            actual: 29,
            required: 30
        })
    );
}

#[test]
fn sortino_rejects_fewer_than_30_days() {
    let returns = repeat("0.01", 29);
    let result = sortino(&returns, Decimal::ZERO);
    assert_eq!(
        result,
        Err(StatsError::InsufficientSample {
            actual: 29,
            required: 30
        })
    );
}

// --- rf is an explicit parameter, zero in the reference profile ---------

#[test]
fn sharpe_with_zero_rf_uses_raw_returns() {
    let mut returns = vec![dec("0.01"); 29];
    returns.push(dec("0.05")); // introduce variance
    let with_zero_rf = sharpe(&returns, Decimal::ZERO).unwrap();
    let with_explicit_zero = sharpe(&returns, dec("0")).unwrap();
    assert_eq!(with_zero_rf, with_explicit_zero);
}

#[test]
fn sharpe_with_nonzero_rf_differs_from_zero_rf() {
    let mut returns = vec![dec("0.01"); 29];
    returns.push(dec("0.05"));
    let zero_rf = sharpe(&returns, Decimal::ZERO).unwrap();
    let nonzero_rf = sharpe(&returns, dec("0.001")).unwrap();
    assert_ne!(
        zero_rf, nonzero_rf,
        "rf must actually shift the excess-return series used"
    );
}

// --- zero variance / no downside: typed reason, never infinite ----------

#[test]
fn sharpe_on_constant_series_is_zero_variance_not_infinite() {
    let returns = repeat("0.01", 30);
    let result = sharpe(&returns, Decimal::ZERO);
    assert_eq!(result, Err(StatsError::ZeroVariance));
}

#[test]
fn sortino_with_no_downside_is_no_downside_not_infinite() {
    // Every return is at or above MAR=0: no downside deviation exists.
    let returns = repeat("0.01", 30);
    let result = sortino(&returns, Decimal::ZERO);
    assert_eq!(result, Err(StatsError::NoDownside));
}

#[test]
fn sortino_with_some_downside_succeeds() {
    let mut returns = repeat("0.01", 29);
    returns.push(dec("-0.02"));
    assert!(sortino(&returns, Decimal::ZERO).is_ok());
}
