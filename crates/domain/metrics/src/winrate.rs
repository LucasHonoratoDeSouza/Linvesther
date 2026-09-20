//! Win rate: the fraction of closed round trips that were profitable.
//!
//! Not part of the original scope — added later once a real,
//! collected trade history (`services/collector/binance/worker`)
//! needed it and no domain crate had it yet. A "round trip" here is a
//! FIFO-matched (buy, sell) pair on the same asset: every sell
//! consumes the *oldest* still-open buy lot(s) first, exactly the
//! accounting convention tax/cost-basis reporting already uses. A
//! partial fill against a lot still closes its own round trip for the
//! matched quantity — this mirrors how a real position is actually
//! realized incrementally, not an approximation.
//!
//! Never a `f64` win/loss threshold at exactly zero P&L either: a
//! round trip that nets to exactly zero is neither a win nor
//! contributes to the denominator's "win" count, but it does still
//! count as a closed round trip — the rate is `wins / closed`, and a
//! breakeven trade is honestly neither.

use rust_decimal::Decimal;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fill {
    pub time_ms: u64,
    pub qty: Decimal,
    pub price: Decimal,
    pub is_buy: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WinRateError {
    #[error("no closed round trips: every buy is still open, or there were no fills at all")]
    NoClosedRoundTrips,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinRateResult {
    pub closed_round_trips: usize,
    pub wins: usize,
    /// `wins / closed_round_trips`, as an exact rational — never
    /// rounded through `f64`.
    pub win_rate: Decimal,
}

/// Computes the FIFO win rate for one asset's chronological fills. A
/// sell with no matching open buy lot (more sold than ever bought in
/// this fill history — e.g. a short, or history starting mid-position)
/// is skipped for the unmatched quantity rather than fabricating a
/// cost basis for it.
pub fn compute_win_rate(fills: &[Fill]) -> Result<WinRateResult, WinRateError> {
    let mut open_lots: VecDeque<(Decimal, Decimal)> = VecDeque::new(); // (qty, price)
    let mut wins = 0usize;
    let mut closed = 0usize;

    let mut ordered: Vec<&Fill> = fills.iter().collect();
    ordered.sort_by_key(|f| f.time_ms);

    for fill in ordered {
        if fill.is_buy {
            open_lots.push_back((fill.qty, fill.price));
            continue;
        }
        let mut remaining = fill.qty;
        while remaining > Decimal::ZERO {
            let Some(&(lot_qty, lot_price)) = open_lots.front() else {
                break; // no open lot left to match; unmatched sell quantity is skipped
            };
            let matched = remaining.min(lot_qty);
            let pnl = (fill.price - lot_price) * matched;
            closed += 1;
            if pnl > Decimal::ZERO {
                wins += 1;
            }
            remaining -= matched;
            let remaining_lot = lot_qty - matched;
            if remaining_lot == Decimal::ZERO {
                open_lots.pop_front();
            } else {
                open_lots[0].0 = remaining_lot;
            }
        }
    }

    if closed == 0 {
        return Err(WinRateError::NoClosedRoundTrips);
    }
    let win_rate = Decimal::from(wins as u64) / Decimal::from(closed as u64);
    Ok(WinRateResult {
        closed_round_trips: closed,
        wins,
        win_rate,
    })
}
