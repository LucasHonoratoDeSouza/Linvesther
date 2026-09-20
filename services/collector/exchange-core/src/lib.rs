//! What the performance engine needs from any exchange, and nothing
//! exchange-specific: balances, market prices and the naming of markets.
//! Binance and Coinbase (and later any broker) each implement
//! [`MarketData`]; the history, PnL and chart code only ever sees this.

use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};

/// One asset's balance on the exchange, as exact decimal strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountBalance {
    pub asset: String,
    pub free: String,
    pub locked: String,
}

/// One price candle: the close, and when it opened and closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCandle {
    pub open_time_ms: u64,
    pub close_price: String,
    pub close_time_ms: u64,
}

/// A failure talking to the exchange — the message is what a person (or
/// the logs) will read, so it says what was being asked and what came back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct MarketError(pub String);

/// Everything the engine reads from an exchange. Implementations must be
/// safe to call from several threads at once (prices are fetched in
/// parallel).
pub trait MarketData: Send + Sync {
    /// The currency every value is expressed in ("USDT", "USD", ...).
    fn quote_currency(&self) -> &str;

    /// The exchange's own name for the market that prices `asset` in the
    /// quote currency (`SOLUSDT`, `SOL-USD`).
    fn market_symbol(&self, asset: &str) -> String;

    /// `(base, quote)` for each of `symbols` the exchange lists. A market
    /// that does not exist is simply absent — that is how "no such market"
    /// (a fiat balance, a delisted pair) is told apart from a failure.
    fn fetch_symbol_assets_for(
        &self,
        symbols: &BTreeSet<String>,
    ) -> Result<BTreeMap<String, (String, String)>, MarketError>;

    fn fetch_account_balances(&self) -> Result<Vec<AccountBalance>, MarketError>;

    /// Every candle of `interval_ms` for `symbol` from `start_ms` to
    /// `end_ms`, oldest first, as few requests as the exchange allows.
    /// `interval_ms` is one of [`SERIES_STEPS_MS`].
    fn fetch_candles_span(
        &self,
        symbol: &str,
        interval_ms: u64,
        start_ms: u64,
        end_ms: u64,
    ) -> Result<Vec<RawCandle>, MarketError>;

    /// The market price of `asset` at exactly `at_ms` (the close of the
    /// last one-minute candle that had closed by then) — what a valuation
    /// at that instant is made of.
    fn price_at_instant(&self, asset: &str, at_ms: u64) -> Result<Decimal, MarketError>;
}

/// The candle sizes a chart may be drawn at — every exchange supports
/// them all (some through a finer size, aggregated).
pub const SERIES_STEPS_MS: [u64; 7] = [
    60_000,
    300_000,
    900_000,
    1_800_000,
    3_600_000,
    7_200_000,
    86_400_000,
];

/// Upper bound on concurrent exchange requests — well inside every
/// exchange's request limits for small public endpoints, while cutting a
/// long chain of ~0.3s round trips down to a few.
const MAX_PARALLEL_REQUESTS: usize = 8;

/// Runs `work` over `items` on up to [`MAX_PARALLEL_REQUESTS`] scoped
/// threads, returning the results in the same order as `items` — so a
/// caller that used to loop sequentially and stop at its first error
/// sees exactly the same first error.
pub fn parallel_map<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    if items.len() <= 1 {
        return items.iter().map(work).collect();
    }
    let threads = MAX_PARALLEL_REQUESTS.min(items.len());
    let chunk_size = items.len().div_ceil(threads);
    let work = &work;
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(chunk_size)
            .map(|chunk| scope.spawn(move || chunk.iter().map(work).collect::<Vec<R>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("an exchange lookup thread panicked"))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::parallel_map;

    #[test]
    fn parallel_map_returns_results_in_input_order_regardless_of_finish_order() {
        // Later items finish first (shorter sleep), yet the output must
        // still line up 1:1 with the input — callers pair each result
        // back to its asset/instant by position.
        let items: Vec<u64> = (0..40).collect();
        let out = parallel_map(&items, |n| {
            std::thread::sleep(std::time::Duration::from_millis(40 - *n));
            n * 2
        });
        assert_eq!(out, items.iter().map(|n| n * 2).collect::<Vec<_>>());
    }

    #[test]
    fn parallel_map_handles_empty_and_single_inputs() {
        assert_eq!(parallel_map(&Vec::<u32>::new(), |n| *n), Vec::<u32>::new());
        assert_eq!(parallel_map(&[7u32], |n| n + 1), vec![8]);
    }
}
