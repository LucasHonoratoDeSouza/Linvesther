//! Pure guest logic for the statistics claims guest: verifying a
//! Sharpe, Sortino or CAGR claim against a certified fixed-point
//! interval, per the protocol specification. This module has no
//! dependency on `risc0_zkvm::guest::env`, so it compiles and is
//! directly unit-testable for the host (native) target, in addition to
//! being the exact code `src/main.rs` runs inside the zkVM — one
//! implementation, so a test exercising it never drifts from what the
//! guest actually proves.

pub mod bracket;
pub mod stats;

use bracket::Bracket;
use serde::{Deserialize, Serialize};
use stats::{
    cagr_bracket, evaluate_claim, sharpe_bracket, sortino_bracket, ClaimOperator, ClaimVerdict,
    StatsError,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricClaim {
    Sharpe {
        returns: Vec<i128>,
        rf_daily: i128,
        operator: ClaimOperator,
    },
    Sortino {
        returns: Vec<i128>,
        mar_daily: i128,
        operator: ClaimOperator,
    },
    Cagr {
        index_start: i128,
        index_end: i128,
        days_elapsed: u32,
        operator: ClaimOperator,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestInput {
    pub claim: MetricClaim,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestOutput {
    pub bracket_lower: i128,
    pub bracket_upper: i128,
    pub verdict: ClaimVerdict,
}

#[derive(Debug)]
pub enum GuestError {
    Stats(StatsError),
}

impl From<StatsError> for GuestError {
    fn from(value: StatsError) -> Self {
        GuestError::Stats(value)
    }
}

pub fn run(input: &GuestInput) -> Result<GuestOutput, GuestError> {
    let (bracket, operator): (Bracket, ClaimOperator) = match &input.claim {
        MetricClaim::Sharpe {
            returns,
            rf_daily,
            operator,
        } => (sharpe_bracket(returns, *rf_daily)?, *operator),
        MetricClaim::Sortino {
            returns,
            mar_daily,
            operator,
        } => (sortino_bracket(returns, *mar_daily)?, *operator),
        MetricClaim::Cagr {
            index_start,
            index_end,
            days_elapsed,
            operator,
        } => (
            cagr_bracket(*index_start, *index_end, *days_elapsed)?,
            *operator,
        ),
    };
    let verdict = evaluate_claim(bracket, operator);
    Ok(GuestOutput {
        bracket_lower: bracket.lower,
        bracket_upper: bracket.upper,
        verdict,
    })
}
