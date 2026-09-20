//! CAGR per the protocol specification: `(I_end/I_start)^(365/d)-1`, `d`
//! in elapsed days; minimum 365 complete gap-free days, index positive.
//!
//! **Known limitation**: the spec requires CAGR to be published as a
//! certified interval `[lower, upper]` of width `<= 10^-10` from a
//! rigorous deterministic root-finding method ("CAGR usa exponenciação/
//! radiciação determinística com limites inferior/superior certificados"),
//! not a single floating/point-style estimate. This module computes a
//! single point value via `rust_decimal`'s `powd` (itself not a certified
//! interval method). Implementing genuine certified interval arithmetic
//! for fractional exponentiation is a real, separate piece of work this
//! delivery does not include — recorded here rather than silently
//! presented as spec-complete.

use rust_decimal::prelude::MathematicalOps;
use rust_decimal::Decimal;

const MIN_CAGR_DAYS: u32 = 365;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CagrError {
    #[error("elapsed days {actual} is fewer than the required minimum {required} complete gap-free days")]
    InsufficientSample { actual: u32, required: u32 },
    #[error("index must be strictly positive at both ends (start={index_start}, end={index_end})")]
    NonPositiveIndex {
        index_start: Decimal,
        index_end: Decimal,
    },
    #[error("arithmetic overflow or invalid range")]
    NumericRange,
}

/// Point-value CAGR (see module limitation note). `days_elapsed` must be
/// at least 365 complete, gap-free days, and both index endpoints must
/// be strictly positive.
pub fn cagr(
    index_start: Decimal,
    index_end: Decimal,
    days_elapsed: u32,
) -> Result<Decimal, CagrError> {
    if days_elapsed < MIN_CAGR_DAYS {
        return Err(CagrError::InsufficientSample {
            actual: days_elapsed,
            required: MIN_CAGR_DAYS,
        });
    }
    if index_start <= Decimal::ZERO || index_end <= Decimal::ZERO {
        return Err(CagrError::NonPositiveIndex {
            index_start,
            index_end,
        });
    }

    let ratio = index_end
        .checked_div(index_start)
        .ok_or(CagrError::NumericRange)?;
    let exponent = Decimal::from(365u32)
        .checked_div(Decimal::from(days_elapsed))
        .ok_or(CagrError::NumericRange)?;
    let powered = ratio
        .checked_powd(exponent)
        .ok_or(CagrError::NumericRange)?;
    powered
        .checked_sub(Decimal::ONE)
        .ok_or(CagrError::NumericRange)
}
