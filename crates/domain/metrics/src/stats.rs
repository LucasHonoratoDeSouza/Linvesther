//! Mean, sample variance, annualized volatility, Sharpe and Sortino, per
//! the protocol specification. Every ratio metric enforces its stated
//! minimum sample size and returns a typed reason instead of publishing
//! an infinite or fabricated value.

use rust_decimal::prelude::MathematicalOps;
use rust_decimal::Decimal;

const DAYS_PER_YEAR: i64 = 365;
const MIN_VOLATILITY_DAYS: usize = 30;
const MIN_SHARPE_SORTINO_DAYS: usize = 30;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StatsError {
    #[error("sample has {actual} daily returns, fewer than the required minimum {required}")]
    InsufficientSample { actual: usize, required: usize },
    #[error("series has zero variance: Sharpe is not publishable, not infinite")]
    ZeroVariance,
    #[error("series has no downside relative to MAR: Sortino is not publishable, not infinite")]
    NoDownside,
    #[error("arithmetic overflow or invalid range")]
    NumericRange,
}

/// `μ = Σr/n`. Requires at least one return.
pub fn mean(returns: &[Decimal]) -> Result<Decimal, StatsError> {
    if returns.is_empty() {
        return Err(StatsError::InsufficientSample {
            actual: 0,
            required: 1,
        });
    }
    let sum: Decimal = returns.iter().copied().sum();
    sum.checked_div(Decimal::from(returns.len() as u64))
        .ok_or(StatsError::NumericRange)
}

/// `s² = Σ(r-μ)²/(n-1)`. Requires at least two returns (sample variance is
/// undefined for `n < 2`).
pub fn sample_variance(returns: &[Decimal]) -> Result<Decimal, StatsError> {
    if returns.len() < 2 {
        return Err(StatsError::InsufficientSample {
            actual: returns.len(),
            required: 2,
        });
    }
    let mu = mean(returns)?;
    let sum_sq: Decimal = returns
        .iter()
        .try_fold(Decimal::ZERO, |acc, r| {
            let diff = r.checked_sub(mu)?;
            let sq = diff.checked_mul(diff)?;
            acc.checked_add(sq)
        })
        .ok_or(StatsError::NumericRange)?;
    sum_sq
        .checked_div(Decimal::from((returns.len() - 1) as u64))
        .ok_or(StatsError::NumericRange)
}

fn sqrt_days_per_year() -> Result<Decimal, StatsError> {
    Decimal::from(DAYS_PER_YEAR)
        .sqrt()
        .ok_or(StatsError::NumericRange)
}

/// `sqrt(365) × s`. Requires at least 30 daily returns.
pub fn annualized_volatility(returns: &[Decimal]) -> Result<Decimal, StatsError> {
    if returns.len() < MIN_VOLATILITY_DAYS {
        return Err(StatsError::InsufficientSample {
            actual: returns.len(),
            required: MIN_VOLATILITY_DAYS,
        });
    }
    let variance = sample_variance(returns)?;
    let stdev = variance.sqrt().ok_or(StatsError::NumericRange)?;
    sqrt_days_per_year()?
        .checked_mul(stdev)
        .ok_or(StatsError::NumericRange)
}

/// `sqrt(365) × mean(r-rf)/stdev_sample(r-rf)`. `rf` is a daily rate
/// (zero in the reference profile, but always an explicit parameter —
/// "rf diário zero no perfil inicial, divulgado"). Requires >= 30 returns
/// and nonzero variance.
pub fn sharpe(returns: &[Decimal], rf_daily: Decimal) -> Result<Decimal, StatsError> {
    if returns.len() < MIN_SHARPE_SORTINO_DAYS {
        return Err(StatsError::InsufficientSample {
            actual: returns.len(),
            required: MIN_SHARPE_SORTINO_DAYS,
        });
    }
    let excess: Vec<Decimal> = returns.iter().map(|r| r - rf_daily).collect();
    let variance = sample_variance(&excess)?;
    if variance == Decimal::ZERO {
        return Err(StatsError::ZeroVariance);
    }
    let stdev = variance.sqrt().ok_or(StatsError::NumericRange)?;
    let mean_excess = mean(&excess)?;
    let ratio = mean_excess
        .checked_div(stdev)
        .ok_or(StatsError::NumericRange)?;
    sqrt_days_per_year()?
        .checked_mul(ratio)
        .ok_or(StatsError::NumericRange)
}

/// `sqrt(365) × mean(r-MAR) / sqrt(Σmin(0,r-MAR)²/n)`. Requires >= 30
/// returns and at least one return below `mar_daily` (downside).
pub fn sortino(returns: &[Decimal], mar_daily: Decimal) -> Result<Decimal, StatsError> {
    if returns.len() < MIN_SHARPE_SORTINO_DAYS {
        return Err(StatsError::InsufficientSample {
            actual: returns.len(),
            required: MIN_SHARPE_SORTINO_DAYS,
        });
    }
    let excess: Vec<Decimal> = returns.iter().map(|r| r - mar_daily).collect();
    let downside_sq_sum: Decimal = excess
        .iter()
        .try_fold(Decimal::ZERO, |acc, e| {
            let downside = if *e < Decimal::ZERO {
                *e
            } else {
                Decimal::ZERO
            };
            acc.checked_add(downside.checked_mul(downside)?)
        })
        .ok_or(StatsError::NumericRange)?;

    if downside_sq_sum == Decimal::ZERO {
        return Err(StatsError::NoDownside);
    }

    let n = Decimal::from(returns.len() as u64);
    let downside_deviation = downside_sq_sum
        .checked_div(n)
        .ok_or(StatsError::NumericRange)?
        .sqrt()
        .ok_or(StatsError::NumericRange)?;
    let mean_excess = mean(&excess)?;
    let ratio = mean_excess
        .checked_div(downside_deviation)
        .ok_or(StatsError::NumericRange)?;
    sqrt_days_per_year()?
        .checked_mul(ratio)
        .ok_or(StatsError::NumericRange)
}
