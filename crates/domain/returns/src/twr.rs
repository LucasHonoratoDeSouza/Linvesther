//! Time-weighted return (TWR) index per the protocol specification:
//!
//! ```text
//! r_j = Vend_j / Vstart_j - 1
//! I_0 = 1; I_j = I_(j-1) × (1+r_j)
//! R[a,b] = I_b / I_a - 1
//! ```
//!
//! One [`compute_twr`] call is one continuous segment. A gap or a
//! non-positive NAV baseline never connects across segments: this
//! function always starts a fresh `I_0 = 1` and has no parameter that
//! could carry a previous segment's index in — "nunca conecta gap ou
//! baseline zero" is a fact about how callers use this function (a gap or
//! a total loss means calling it again, fresh), not a special case
//! inside it.
//!
//! Input is a chronological stream of [`TimelineEvent`]s:
//! - `Valuation { nav }`: an authenticated NAV observation. If a prior NAV
//!   is already known (from an earlier `Valuation` or the aftermath of a
//!   `Flow`), it closes the current subperiod as `Vend`, computing and
//!   chaining `r_j` — then this same `nav` becomes `Vstart` for the next
//!   subperiod. The very first event of a segment, if a `Valuation`,
//!   establishes the baseline without closing anything.
//! - `Flow { amount }`: `Vafter = Vbefore + amount` (deposit positive,
//!   withdrawal negative), per the spec — the jump does **not** alter the
//!   index. `Vbefore` is the last known NAV (zero if this is the very
//!   first event, i.e. capital arriving into an empty segment).
//!
//! The last subperiod is only counted if the event stream ends with a
//! closing `Valuation` — an event stream that ends right after a `Flow`
//! (e.g. a full withdrawal to zero) leaves that final subperiod
//! deliberately uncounted, which is exactly correct: a full withdrawal to
//! zero is not a P&L loss and must not produce a -100% subperiod.

use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineEvent {
    Valuation { time_ms: u64, nav: Decimal },
    Flow { time_ms: u64, amount: Decimal },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReturnError {
    #[error("subperiod baseline is not positive ({v_start}): no return until the next positive baseline")]
    NonPositiveBaseline { v_start: Decimal },
    #[error("arithmetic overflow or invalid range while computing the index")]
    NumericRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwrResult {
    /// `I_j`: 1 at the segment's own baseline, growing/shrinking
    /// multiplicatively with each subperiod's return.
    pub index: Decimal,
    /// `r_j` for each closed subperiod, in order, for audit — e.g. to
    /// derive daily returns from finer-grained events.
    pub sub_returns: Vec<Decimal>,
}

impl TwrResult {
    /// `R[a,b] = I_b / I_a - 1` between two points in the *same* segment
    /// (the caller is responsible for not comparing across a gap; see
    /// module docs — this crate does not track segment identity).
    pub fn cumulative_return(index_a: Decimal, index_b: Decimal) -> Result<Decimal, ReturnError> {
        if index_a <= Decimal::ZERO {
            return Err(ReturnError::NonPositiveBaseline { v_start: index_a });
        }
        checked_ratio_minus_one(index_b, index_a)
    }
}

/// Computes the TWR index for one continuous segment from a chronological
/// event stream. See module docs for the exact semantics of each event
/// kind and of the final (possibly uncounted) subperiod.
pub fn compute_twr(events: &[TimelineEvent]) -> Result<TwrResult, ReturnError> {
    let mut index = Decimal::ONE;
    let mut sub_returns = Vec::new();
    let mut current_nav: Option<Decimal> = None;

    for event in events {
        match *event {
            TimelineEvent::Valuation { nav, .. } => {
                if let Some(v_start) = current_nav {
                    if v_start <= Decimal::ZERO {
                        return Err(ReturnError::NonPositiveBaseline { v_start });
                    }
                    let r = checked_ratio_minus_one(nav, v_start)?;
                    index = chain(index, r)?;
                    sub_returns.push(r);
                }
                current_nav = Some(nav);
            }
            TimelineEvent::Flow { amount, .. } => {
                let before = current_nav.unwrap_or(Decimal::ZERO);
                let after = before
                    .checked_add(amount)
                    .ok_or(ReturnError::NumericRange)?;
                current_nav = Some(after);
            }
        }
    }

    Ok(TwrResult { index, sub_returns })
}

fn checked_ratio_minus_one(
    numerator: Decimal,
    denominator: Decimal,
) -> Result<Decimal, ReturnError> {
    if denominator <= Decimal::ZERO {
        return Err(ReturnError::NonPositiveBaseline {
            v_start: denominator,
        });
    }
    let ratio = numerator
        .checked_div(denominator)
        .ok_or(ReturnError::NumericRange)?;
    ratio
        .checked_sub(Decimal::ONE)
        .ok_or(ReturnError::NumericRange)
}

fn chain(index: Decimal, r: Decimal) -> Result<Decimal, ReturnError> {
    let factor = Decimal::ONE
        .checked_add(r)
        .ok_or(ReturnError::NumericRange)?;
    index.checked_mul(factor).ok_or(ReturnError::NumericRange)
}
