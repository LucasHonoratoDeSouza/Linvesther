//! Binance as one implementation of [`exchange_core::MarketData`] — the
//! only thing the history, PnL and chart code know about an exchange.

use binance_live_client::{LiveClient, LiveClientError};
use exchange_core::{AccountBalance, MarketData, MarketError, RawCandle};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

/// Every Binance value is in USDT.
pub const QUOTE_CURRENCY: &str = "USDT";

pub struct BinanceMarket {
    client: LiveClient,
}

impl BinanceMarket {
    pub fn new(client: LiveClient) -> Self {
        BinanceMarket { client }
    }
}

fn failed(what: &str, error: LiveClientError) -> MarketError {
    MarketError(format!("Binance {what}: {error}"))
}

/// Binance's own name for each candle size the engine draws charts at.
fn interval_label(interval_ms: u64) -> Result<&'static str, MarketError> {
    Ok(match interval_ms {
        60_000 => "1m",
        300_000 => "5m",
        900_000 => "15m",
        1_800_000 => "30m",
        3_600_000 => "1h",
        7_200_000 => "2h",
        86_400_000 => "1d",
        other => return Err(MarketError(format!("no Binance candle size of {other} ms"))),
    })
}

impl MarketData for BinanceMarket {
    fn quote_currency(&self) -> &str {
        QUOTE_CURRENCY
    }

    fn market_symbol(&self, asset: &str) -> String {
        format!("{asset}{QUOTE_CURRENCY}")
    }

    fn fetch_symbol_assets_for(
        &self,
        symbols: &BTreeSet<String>,
    ) -> Result<BTreeMap<String, (String, String)>, MarketError> {
        self.client
            .fetch_symbol_assets_for(symbols)
            .map_err(|e| failed("symbol lookup", e))
    }

    fn fetch_account_balances(&self) -> Result<Vec<AccountBalance>, MarketError> {
        self.client
            .fetch_account_balances()
            .map_err(|e| failed("balances", e))
    }

    fn fetch_candles_span(
        &self,
        symbol: &str,
        interval_ms: u64,
        start_ms: u64,
        end_ms: u64,
    ) -> Result<Vec<RawCandle>, MarketError> {
        self.client
            .fetch_klines_span(symbol, interval_label(interval_ms)?, start_ms, end_ms)
            .map_err(|e| failed(&format!("{symbol} candles"), e))
    }

    /// An asset with no market against USDT at all (e.g. a fiat balance
    /// like BRL, which has no BRLUSDT spot pair) has Binance reject the
    /// symbol outright (-1121 "Invalid symbol") rather than return an
    /// empty candle list — that is itself "no price support". Passing
    /// only `endTime` (no `startTime`) is what makes Binance return the
    /// `limit` candles immediately preceding that instant — confirmed
    /// against the real API: combining a wide `startTime` with a small
    /// `limit` instead returns candles from the *start* of that window.
    fn price_at_instant(&self, asset: &str, at_ms: u64) -> Result<Decimal, MarketError> {
        let symbol = self.market_symbol(asset);
        let raw_candles = self
            .client
            .fetch_klines_in_range(&symbol, "1m", 5, None, Some(at_ms))
            .map_err(|e| failed(&format!("{symbol} price"), e))?;
        let candles: Vec<binance_prices::Candle> = raw_candles
            .into_iter()
            .map(|c: RawCandle| binance_prices::Candle {
                symbol: symbol.clone(),
                open_time_ms: c.open_time_ms,
                close_time_ms: c.close_time_ms,
                close_price: c.close_price,
                is_closed: c.close_time_ms <= at_ms,
            })
            .collect();
        let mark = binance_prices::mark_from_series(
            &symbol,
            &candles,
            at_ms,
            &binance_prices::PricePolicy::reference(),
        )
        .map_err(|e| MarketError(format!("{symbol}: {e}")))?;
        Decimal::from_str(&mark.price).map_err(|_| MarketError(format!("{symbol}: invalid price {}", mark.price)))
    }
}
