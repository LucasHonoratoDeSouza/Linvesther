//! Sharpe, Sortino and CAGR as certified brackets, and the claim
//! comparison rule from the protocol specification: "Claims usam
//! valores internos/intervalos certificados... Intervalo que cruza o
//! limiar produz INDETERMINATE_PRECISION."
//!
//! `returns`/`rf_daily`/`mar_daily`/indices are all `SCALE`-scaled
//! fixed-point integers (see [`crate::bracket::SCALE`]) — the same
//! representation `zkvm/methods/performance`'s guest uses for NAV.
//!
//! **CAGR's bracket** is not computed by directly evaluating
//! `ratio^(365/days_elapsed)`: clearing the fraction (comparing
//! `y^days_elapsed` against `ratio^365`) requires raising either side
//! to an exponent in the hundreds or thousands, which overflows any
//! fixed-width integer for realistic multi-year returns long before it
//! says anything about CAGR itself (a bounded, modest quantity). This
//! module instead expands the exponent `365/days_elapsed` (always in
//! `(0, 1]`) into its binary fraction and multiplies together the
//! iterated-square-root terms of `ratio` selected by each set bit —
//! `ratio^(b_1/2 + b_2/4 + ... )` — which stays bounded throughout
//! because every factor is between `min(1, ratio)` and `max(1, ratio)`.
//! [`CAGR_EXPANSION_BITS`] bits gives convergence well past `SCALE`'s
//! own precision floor.

use crate::bracket::{
    mul_bracket_nonneg, ratio_bracket, sqrt_bracket_scaled, sqrt_of_bracket, Bracket, SCALE,
};
use serde::{Deserialize, Serialize};

const MIN_SHARPE_SORTINO_DAYS: usize = 30;
const MIN_CAGR_DAYS: u32 = 365;
const DAYS_PER_YEAR: i128 = 365;
const CAGR_EXPANSION_BITS: u32 = 40;

#[derive(Debug, PartialEq, Eq)]
pub enum StatsError {
    InsufficientSample { actual: usize, required: usize },
    ZeroVariance,
    NoDownside,
    NonPositiveIndex,
    NumericRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimOperator {
    GreaterThan(i128),
    LessThan(i128),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimVerdict {
    Approved,
    Rejected,
    IndeterminatePrecision,
}

/// `metric > k` approves only if `lower > k`; `metric < k` approves
/// only if `upper < k`. An interval that straddles the threshold is
/// neither a clean approval nor a clean rejection.
pub fn evaluate_claim(bracket: Bracket, claim: ClaimOperator) -> ClaimVerdict {
    match claim {
        ClaimOperator::GreaterThan(k) => {
            if bracket.lower > k {
                ClaimVerdict::Approved
            } else if bracket.upper <= k {
                ClaimVerdict::Rejected
            } else {
                ClaimVerdict::IndeterminatePrecision
            }
        }
        ClaimOperator::LessThan(k) => {
            if bracket.upper < k {
                ClaimVerdict::Approved
            } else if bracket.lower >= k {
                ClaimVerdict::Rejected
            } else {
                ClaimVerdict::IndeterminatePrecision
            }
        }
    }
}

fn sum_checked(values: &[i128]) -> Result<i128, StatsError> {
    values
        .iter()
        .try_fold(0i128, |acc, v| acc.checked_add(*v))
        .ok_or(StatsError::NumericRange)
}

fn annualization_factor() -> Result<Bracket, StatsError> {
    sqrt_bracket_scaled(DAYS_PER_YEAR, 1, SCALE).ok_or(StatsError::NumericRange)
}

/// `sqrt(365) * mean(r-rf) / stdev_sample(r-rf)`. Requires >= 30 daily
/// returns and nonzero variance — never publishes zero or infinity.
pub fn sharpe_bracket(returns: &[i128], rf_daily: i128) -> Result<Bracket, StatsError> {
    if returns.len() < MIN_SHARPE_SORTINO_DAYS {
        return Err(StatsError::InsufficientSample {
            actual: returns.len(),
            required: MIN_SHARPE_SORTINO_DAYS,
        });
    }
    let n = returns.len() as i128;
    let excess: Vec<i128> = returns.iter().map(|r| r - rf_daily).collect();
    let sum_excess = sum_checked(&excess)?;

    let mut v_num: i128 = 0;
    for e in &excess {
        let term = e
            .checked_mul(n)
            .ok_or(StatsError::NumericRange)?
            .checked_sub(sum_excess)
            .ok_or(StatsError::NumericRange)?;
        let sq = term.checked_mul(term).ok_or(StatsError::NumericRange)?;
        v_num = v_num.checked_add(sq).ok_or(StatsError::NumericRange)?;
    }
    if v_num == 0 {
        return Err(StatsError::ZeroVariance);
    }
    let v_den = n
        .checked_mul(n)
        .ok_or(StatsError::NumericRange)?
        .checked_mul(n - 1)
        .ok_or(StatsError::NumericRange)?;

    let stdev = sqrt_bracket_scaled(v_num, v_den, 1).ok_or(StatsError::NumericRange)?;
    let annual = annualization_factor()?;
    ratio_bracket(annual, sum_excess, n, stdev).ok_or(StatsError::NumericRange)
}

/// `sqrt(365) * mean(r-MAR) / sqrt(mean(min(0,r-MAR)^2))`. Requires >=
/// 30 daily returns and at least one return below `mar_daily`.
pub fn sortino_bracket(returns: &[i128], mar_daily: i128) -> Result<Bracket, StatsError> {
    if returns.len() < MIN_SHARPE_SORTINO_DAYS {
        return Err(StatsError::InsufficientSample {
            actual: returns.len(),
            required: MIN_SHARPE_SORTINO_DAYS,
        });
    }
    let n = returns.len() as i128;
    let excess: Vec<i128> = returns.iter().map(|r| r - mar_daily).collect();
    let sum_excess = sum_checked(&excess)?;

    let mut downside_sq_sum: i128 = 0;
    for e in &excess {
        if *e < 0 {
            let sq = e.checked_mul(*e).ok_or(StatsError::NumericRange)?;
            downside_sq_sum = downside_sq_sum
                .checked_add(sq)
                .ok_or(StatsError::NumericRange)?;
        }
    }
    if downside_sq_sum == 0 {
        return Err(StatsError::NoDownside);
    }

    let downside_dev =
        sqrt_bracket_scaled(downside_sq_sum, n, 1).ok_or(StatsError::NumericRange)?;
    let annual = annualization_factor()?;
    ratio_bracket(annual, sum_excess, n, downside_dev).ok_or(StatsError::NumericRange)
}

/// `(index_end/index_start)^(365/days_elapsed) - 1`. Requires at least
/// 365 complete, gap-free days and a strictly positive index at both
/// ends — see this module's doc comment for the certified-interval
/// method.
pub fn cagr_bracket(
    index_start: i128,
    index_end: i128,
    days_elapsed: u32,
) -> Result<Bracket, StatsError> {
    if days_elapsed < MIN_CAGR_DAYS {
        return Err(StatsError::InsufficientSample {
            actual: days_elapsed as usize,
            required: MIN_CAGR_DAYS as usize,
        });
    }
    if index_start <= 0 || index_end <= 0 {
        return Err(StatsError::NonPositiveIndex);
    }

    let scaled = index_end
        .checked_mul(SCALE)
        .ok_or(StatsError::NumericRange)?;
    let ratio_lower = scaled / index_start;
    let ratio_upper = if scaled % index_start != 0 {
        ratio_lower + 1
    } else {
        ratio_lower
    };
    let ratio = Bracket {
        lower: ratio_lower,
        upper: ratio_upper,
    };

    let mut term = sqrt_of_bracket(ratio).ok_or(StatsError::NumericRange)?; // ratio^(2^-1)
    let mut result = Bracket::exact(SCALE); // running product, starts at 1.0
    let mut remaining_num = DAYS_PER_YEAR;
    let den = days_elapsed as i128;

    for _ in 0..CAGR_EXPANSION_BITS {
        remaining_num = remaining_num
            .checked_mul(2)
            .ok_or(StatsError::NumericRange)?;
        if remaining_num >= den {
            remaining_num -= den;
            result = mul_bracket_nonneg(result, term).ok_or(StatsError::NumericRange)?;
        }
        term = sqrt_of_bracket(term).ok_or(StatsError::NumericRange)?;
    }

    Ok(Bracket {
        lower: result.lower - SCALE,
        upper: result.upper - SCALE,
    })
}
