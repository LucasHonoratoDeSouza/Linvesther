//! Resolves an authenticated valuation mark from candle data, per
//! the protocol specification's reference price policy: closed
//! 1-minute candle, age ≤ 120 s, never a future candle, never a zero
//! fallback.

use crate::candle::Candle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricePolicy {
    pub max_age_ms: u64,
}

impl PricePolicy {
    /// The reference profile: 120 s maximum age.
    pub fn reference() -> Self {
        Self {
            max_age_ms: 120_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub symbol: String,
    pub price: String,
    pub candle_close_time_ms: u64,
    pub evaluated_at_ms: u64,
    pub age_ms: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MarkError {
    #[error("no candle available to mark {symbol} at {evaluated_at_ms}")]
    MissingPrice {
        symbol: String,
        evaluated_at_ms: u64,
    },
    #[error("candle for {symbol} is not yet closed")]
    CandleNotClosed { symbol: String },
    #[error("candle for {symbol} closes at {candle_close_time_ms}, after the evaluation instant {evaluated_at_ms}: a future candle")]
    CandleInTheFuture {
        symbol: String,
        evaluated_at_ms: u64,
        candle_close_time_ms: u64,
    },
    #[error("candle for {symbol} is {age_ms}ms old, exceeding the {max_age_ms}ms policy limit")]
    CandleTooOld {
        symbol: String,
        age_ms: u64,
        max_age_ms: u64,
    },
}

/// Picks the most recent *closed* candle at or before `evaluated_at_ms`
/// from `candles` — never an in-progress or future one. Returns `None`
/// (not a fallback to the newest candle regardless of timing) when no
/// candle qualifies.
pub fn select_candle(candles: &[Candle], evaluated_at_ms: u64) -> Option<&Candle> {
    candles
        .iter()
        .filter(|c| c.is_closed && c.close_time_ms <= evaluated_at_ms)
        .max_by_key(|c| c.close_time_ms)
}

/// Resolves a [`Mark`] from an already-selected candle (typically the
/// output of [`select_candle`]). Every rejection path returns an
/// [`MarkError`]; there is no code path in this function that produces a
/// `Mark` with an estimated, zero, or otherwise non-authentic price —
/// "preço ausente/futuro nunca vira zero" is enforced by this function
/// simply having no such branch, not by a runtime check.
pub fn resolve_mark(
    symbol: &str,
    candle: Option<&Candle>,
    evaluated_at_ms: u64,
    policy: &PricePolicy,
) -> Result<Mark, MarkError> {
    let candle = candle.ok_or_else(|| MarkError::MissingPrice {
        symbol: symbol.to_string(),
        evaluated_at_ms,
    })?;

    if !candle.is_closed {
        return Err(MarkError::CandleNotClosed {
            symbol: symbol.to_string(),
        });
    }
    if candle.close_time_ms > evaluated_at_ms {
        return Err(MarkError::CandleInTheFuture {
            symbol: symbol.to_string(),
            evaluated_at_ms,
            candle_close_time_ms: candle.close_time_ms,
        });
    }

    let age_ms = evaluated_at_ms - candle.close_time_ms;
    if age_ms > policy.max_age_ms {
        return Err(MarkError::CandleTooOld {
            symbol: symbol.to_string(),
            age_ms,
            max_age_ms: policy.max_age_ms,
        });
    }

    Ok(Mark {
        symbol: symbol.to_string(),
        price: candle.close_price.clone(),
        candle_close_time_ms: candle.close_time_ms,
        evaluated_at_ms,
        age_ms,
    })
}

/// Convenience: selects the best candle from `candles` and resolves it in
/// one call.
pub fn mark_from_series(
    symbol: &str,
    candles: &[Candle],
    evaluated_at_ms: u64,
    policy: &PricePolicy,
) -> Result<Mark, MarkError> {
    resolve_mark(
        symbol,
        select_candle(candles, evaluated_at_ms),
        evaluated_at_ms,
        policy,
    )
}
