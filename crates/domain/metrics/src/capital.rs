//! Capital atual / capital sustentado / consistência, per
//! the protocol specification.

use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentCapital {
    pub nav: Decimal,
    pub as_of_ms: u64,
    pub age_ms: u64,
}

/// `Capital atual`: NAV at the last eligible cut, with its timestamp and
/// age relative to `now_ms`.
pub fn current_capital(nav: Decimal, as_of_ms: u64, now_ms: u64) -> CurrentCapital {
    CurrentCapital {
        nav,
        as_of_ms,
        age_ms: now_ms.saturating_sub(as_of_ms),
    }
}

/// `Capital sustentado`: the minimum NAV across daily closes of a
/// continuous period **and the baseline** — never just the daily closes
/// alone, and never an intraday minimum (this function only ever sees
/// daily-close-granularity values, by its own type signature).
pub fn sustained_capital(baseline: Decimal, daily_closes: &[Decimal]) -> Decimal {
    daily_closes.iter().copied().fold(baseline, Decimal::min)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Consistency {
    pub positive_periods: u64,
    pub total_periods: u64,
}

impl Consistency {
    /// `None` if `total_periods == 0` (denominator not positive) —
    /// matches the protocol's general rule of never dividing by zero for
    /// a metric; a caller with no complete periods has no consistency
    /// figure to publish, not a `0/0` treated as `0`.
    pub fn ratio(&self) -> Option<Decimal> {
        if self.total_periods == 0 {
            return None;
        }
        Decimal::from(self.positive_periods).checked_div(Decimal::from(self.total_periods))
    }
}

/// Fraction of complete calendar periods (days/weeks/months/years — the
/// caller decides the bucketing and supplies one return per complete
/// period) with return strictly greater than zero: "zero não é positivo."
pub fn consistency(period_returns: &[Decimal]) -> Consistency {
    let positive_periods = period_returns
        .iter()
        .filter(|r| **r > Decimal::ZERO)
        .count() as u64;
    Consistency {
        positive_periods,
        total_periods: period_returns.len() as u64,
    }
}
