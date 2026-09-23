//! The market a wallet is valued in: dollars, from DefiLlama, for the assets the address holds.
//! It gives the engine what an exchange would — balances, a market per asset, prices at a moment
//! and over a range — with the balances being the address's as of its last final block.

use crate::chains::price_id;
use crate::prices::PriceProvider;
use exchange_core::{AccountBalance, MarketData, MarketError, RawCandle};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

pub const QUOTE_CURRENCY: &str = "USD";

pub struct WalletMarket {
    prices: Arc<dyn PriceProvider>,
    balances: Vec<AccountBalance>,
    /// Which followed assets have a price now, found once.
    priced: Mutex<Option<BTreeSet<String>>>,
}

impl WalletMarket {
    /// `balances` are the address's balances of every followed asset, zero included.
    pub fn new(prices: Arc<dyn PriceProvider>, balances: Vec<AccountBalance>) -> Self {
        WalletMarket { prices, balances, priced: Mutex::new(None) }
    }

    /// The followed assets that can be priced now.
    pub fn priced_assets(&self) -> Result<BTreeSet<String>, MarketError> {
        let mut held = self.priced.lock().expect("the price cache lock is not poisoned");
        if let Some(found) = held.as_ref() {
            return Ok(found.clone());
        }
        let ids: Vec<(String, String)> = self.balances.iter().filter_map(|b| price_id(&b.asset).map(|id| (b.asset.clone(), id))).collect();
        let found = self.prices.current(&ids.iter().map(|(_, id)| id.clone()).collect::<Vec<_>>()).map_err(|e| MarketError(e.to_string()))?;
        let priced: BTreeSet<String> = ids.into_iter().filter(|(_, id)| found.contains_key(id)).map(|(asset, _)| asset).collect();
        *held = Some(priced.clone());
        Ok(priced)
    }

    fn price_id_of(asset: &str) -> Result<String, MarketError> {
        price_id(asset).ok_or_else(|| MarketError(format!("{asset} is not an asset that can be priced")))
    }
}

impl MarketData for WalletMarket {
    fn quote_currency(&self) -> &str {
        QUOTE_CURRENCY
    }

    fn market_symbol(&self, asset: &str) -> String {
        format!("{asset}-{QUOTE_CURRENCY}")
    }

    fn fetch_symbol_assets_for(&self, symbols: &BTreeSet<String>) -> Result<BTreeMap<String, (String, String)>, MarketError> {
        let priced = self.priced_assets()?;
        Ok(symbols
            .iter()
            .filter_map(|symbol| {
                let asset = symbol.strip_suffix(&format!("-{QUOTE_CURRENCY}"))?;
                priced.contains(asset).then(|| (symbol.clone(), (asset.to_string(), QUOTE_CURRENCY.to_string())))
            })
            .collect())
    }

    fn fetch_account_balances(&self) -> Result<Vec<AccountBalance>, MarketError> {
        Ok(self.balances.clone())
    }

    /// Price points as candles: each closes at the moment the price was observed and holds the price
    /// from the previous interval's end. The source does not go finer than five minutes.
    fn fetch_candles_span(&self, symbol: &str, interval_ms: u64, start_ms: u64, end_ms: u64) -> Result<Vec<RawCandle>, MarketError> {
        let asset = symbol.strip_suffix(&format!("-{QUOTE_CURRENCY}")).ok_or_else(|| MarketError(format!("{symbol} is not a market of this wallet")))?;
        let id = Self::price_id_of(asset)?;
        let points = self.prices.chart(&id, start_ms, end_ms, interval_ms).map_err(|e| MarketError(e.to_string()))?;
        Ok(points.into_iter().map(|(at_ms, price)| RawCandle { open_time_ms: at_ms.saturating_sub(interval_ms) + 1, close_time_ms: at_ms, close_price: price.to_string() }).collect())
    }

    fn price_at_instant(&self, asset: &str, at_ms: u64) -> Result<Decimal, MarketError> {
        let id = Self::price_id_of(asset)?;
        let found = self.prices.at(std::slice::from_ref(&id), at_ms).map_err(|e| MarketError(e.to_string()))?;
        found.get(&id).map(|p| p.usd).ok_or_else(|| MarketError(format!("{asset}: no price near {at_ms}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::SyntheticPrices;
    use std::str::FromStr;

    fn balance(asset: &str) -> AccountBalance {
        AccountBalance { asset: asset.into(), free: "1".into(), locked: "0".into() }
    }

    fn market() -> WalletMarket {
        let prices = SyntheticPrices::new();
        prices.set("coingecko:ethereum", Decimal::from_str("2600").unwrap());
        prices.set("base:0xusdc", Decimal::ONE);
        WalletMarket::new(Arc::new(prices), vec![balance("ethereum:native"), balance("base:0xusdc"), balance("base:0xspam")])
    }

    #[test]
    fn a_market_exists_for_each_priced_asset_and_only_those() {
        let market = market();
        let symbols: BTreeSet<String> = ["ethereum:native-USD", "base:0xusdc-USD", "base:0xspam-USD", "base:0xnotheld-USD", "garbage"].map(String::from).into();
        let found = market.fetch_symbol_assets_for(&symbols).unwrap();
        assert_eq!(found.keys().cloned().collect::<Vec<_>>(), vec!["base:0xusdc-USD", "ethereum:native-USD"]);
        assert_eq!(found["base:0xusdc-USD"], ("base:0xusdc".to_string(), "USD".to_string()));
        assert_eq!(market.market_symbol("base:0xusdc"), "base:0xusdc-USD");
    }

    #[test]
    fn prices_come_at_the_moment_asked_and_a_missing_one_is_an_error() {
        let market = market();
        assert_eq!(market.price_at_instant("ethereum:native", 1_700_000_000_000).unwrap().to_string(), "2600");
        assert!(market.price_at_instant("base:0xspam", 1).is_err());
        assert!(market.price_at_instant("bogus", 1).is_err());
    }

    #[test]
    fn a_range_of_prices_becomes_candles_that_close_when_the_price_was_seen() {
        let market = market();
        let candles = market.fetch_candles_span("base:0xusdc-USD", 300_000, 1_000_000, 1_900_000).unwrap();
        assert_eq!(candles.iter().map(|c| c.close_time_ms).collect::<Vec<_>>(), vec![1_000_000, 1_300_000, 1_600_000, 1_900_000]);
        assert!(candles.iter().all(|c| c.open_time_ms < c.close_time_ms && c.close_price == "1"));
    }

    #[test]
    fn the_balances_given_are_the_balances_returned() {
        assert_eq!(market().fetch_account_balances().unwrap().len(), 3);
    }
}
