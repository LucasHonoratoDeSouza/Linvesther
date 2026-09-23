//! Token prices in dollars from DefiLlama's coins API (no key needed): the price now, at a past
//! moment, and over a range. A coin it has no price for is simply absent from the answer, which
//! is how "no price" is told apart from a failure. Coins are named `chain:0xcontract` or
//! `coingecko:id`.

use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const DEFAULT_BASE_URL: &str = "https://coins.llama.fi";
/// Coins named in one request, well inside a URL's length limit.
const COINS_PER_REQUEST: usize = 40;
/// Points asked for in one chart request.
const POINTS_PER_CHART: u64 = 500;
/// A price DefiLlama itself is unsure of is treated as no price.
const MIN_CONFIDENCE: f64 = 0.5;
const RATE_LIMIT_RETRIES: u32 = 3;

#[derive(Debug, thiserror::Error)]
pub enum PriceError {
    #[error("could not reach the price source: {0}")]
    Network(String),
    #[error("the price source sent something unexpected: {0}")]
    Unexpected(String),
    #[error("the price source's rate limit was reached")]
    RateLimited,
}

/// A price and the moment it was observed.
#[derive(Debug, Clone, PartialEq)]
pub struct Price {
    pub usd: Decimal,
    pub at_ms: u64,
}

pub struct PriceSource {
    base_url: String,
    http: reqwest::blocking::Client,
    spacing: Duration,
    retry_wait: Duration,
    next_call: Mutex<Instant>,
}

impl Default for PriceSource {
    fn default() -> Self {
        Self::new(DEFAULT_BASE_URL)
    }
}

/// A JSON number as an exact decimal (prices can come in scientific notation).
fn decimal_of(value: &Value) -> Option<Decimal> {
    let text = value.as_f64().map(|_| value.to_string())?;
    Decimal::from_str_exact(&text).or_else(|_| Decimal::from_scientific(&text)).ok()
}

/// The period DefiLlama names for a candle size in milliseconds; sizes finer than its finest
/// (five minutes) are drawn at five.
fn period_of(interval_ms: u64) -> String {
    match interval_ms {
        0..=300_000 => "5m".into(),
        900_000 => "15m".into(),
        1_800_000 => "30m".into(),
        3_600_000 => "1h".into(),
        7_200_000 => "2h".into(),
        86_400_000 => "1d".into(),
        other => format!("{}m", other / 60_000),
    }
}

impl PriceSource {
    pub fn new(base_url: impl Into<String>) -> Self {
        PriceSource {
            base_url: base_url.into(),
            http: reqwest::blocking::Client::builder().user_agent("linvesther-collector").timeout(Duration::from_secs(30)).build().expect("the HTTP client can be built"),
            spacing: Duration::from_millis(150),
            retry_wait: Duration::from_secs(2),
            next_call: Mutex::new(Instant::now()),
        }
    }

    /// Faster pacing, for a test.
    pub fn with_pacing(mut self, spacing: Duration, retry_wait: Duration) -> Self {
        self.spacing = spacing;
        self.retry_wait = retry_wait;
        self
    }

    fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, PriceError> {
        for attempt in 0..=RATE_LIMIT_RETRIES {
            let wait = {
                let mut next = self.next_call.lock().expect("the spacing lock is not poisoned");
                let slot = (*next).max(Instant::now());
                *next = slot + self.spacing;
                slot.saturating_duration_since(Instant::now())
            };
            std::thread::sleep(wait);
            let response = self.http.get(format!("{}{path}", self.base_url)).query(query).send().map_err(|e| PriceError::Network(e.without_url().to_string()))?;
            let status = response.status();
            if status.as_u16() == 429 {
                if attempt < RATE_LIMIT_RETRIES {
                    std::thread::sleep(self.retry_wait * (attempt + 1));
                    continue;
                }
                return Err(PriceError::RateLimited);
            }
            let text = response.text().map_err(|e| PriceError::Network(e.without_url().to_string()))?;
            if !status.is_success() {
                return Err(PriceError::Unexpected(format!("answered {status}")));
            }
            return serde_json::from_str(&text).map_err(|_| PriceError::Unexpected("the answer was not JSON".into()));
        }
        unreachable!("the loop returns on its last attempt")
    }

    fn priced(coin: &Value) -> Option<Price> {
        if coin.get("confidence").and_then(Value::as_f64).is_some_and(|c| c < MIN_CONFIDENCE) {
            return None;
        }
        Some(Price { usd: decimal_of(coin.get("price")?)?, at_ms: coin.get("timestamp")?.as_u64()? * 1000 })
    }

    fn batch(&self, path_prefix: &str, coins: &[String], query: &[(&str, String)]) -> Result<BTreeMap<String, Price>, PriceError> {
        let mut out = BTreeMap::new();
        for group in coins.chunks(COINS_PER_REQUEST) {
            let body = self.get(&format!("{path_prefix}{}", group.join(",")), query)?;
            let found = body.get("coins").and_then(Value::as_object).ok_or_else(|| PriceError::Unexpected("no coins in the answer".into()))?;
            for (coin, entry) in found {
                if let Some(price) = Self::priced(entry) {
                    out.insert(coin.clone(), price);
                }
            }
        }
        Ok(out)
    }

    /// The price of each coin now. A coin without a price is left out.
    pub fn current(&self, coins: &[String]) -> Result<BTreeMap<String, Price>, PriceError> {
        self.batch("/prices/current/", coins, &[])
    }

    /// The price of each coin at `at_ms`, from an observation within two hours of it. A coin
    /// without one is left out.
    pub fn at(&self, coins: &[String], at_ms: u64) -> Result<BTreeMap<String, Price>, PriceError> {
        self.batch(&format!("/prices/historical/{}/", at_ms / 1000), coins, &[("searchWidth", "2h".into())])
    }

    /// Prices of one coin from `start_ms` to `end_ms`, oldest first, as points `interval_ms` apart
    /// (five minutes at the finest). Points the source lacks are simply missing.
    pub fn chart(&self, coin: &str, start_ms: u64, end_ms: u64, interval_ms: u64) -> Result<Vec<(u64, Decimal)>, PriceError> {
        let period = period_of(interval_ms);
        let step_ms = interval_ms.max(300_000);
        let mut points: BTreeMap<u64, Decimal> = BTreeMap::new();
        let mut from = start_ms;
        while from <= end_ms {
            let span = ((end_ms - from) / step_ms + 1).min(POINTS_PER_CHART);
            let body = self.get(&format!("/chart/{coin}"), &[("start", (from / 1000).to_string()), ("span", span.to_string()), ("period", period.clone())])?;
            let series = body.get("coins").and_then(|c| c.get(coin)).and_then(|c| c.get("prices")).and_then(Value::as_array);
            for point in series.into_iter().flatten() {
                if let (Some(at), Some(price)) = (point.get("timestamp").and_then(Value::as_u64), point.get("price").and_then(decimal_of)) {
                    points.insert(at * 1000, price);
                }
            }
            from += span * step_ms;
        }
        Ok(points.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn prices_are_exact_even_in_scientific_notation() {
        assert_eq!(decimal_of(&json!(0.9998930566516456)).unwrap().to_string(), "0.9998930566516456");
        assert_eq!(decimal_of(&json!(2.5e-7)).unwrap().to_string(), "0.00000025");
        assert!(decimal_of(&json!("not a number")).is_none());
    }

    #[test]
    fn a_low_confidence_price_counts_as_no_price() {
        assert!(PriceSource::priced(&json!({ "price": 1.0, "timestamp": 10, "confidence": 0.2 })).is_none());
        let price = PriceSource::priced(&json!({ "price": 1.5, "timestamp": 10, "confidence": 0.99 })).unwrap();
        assert_eq!((price.usd.to_string(), price.at_ms), ("1.5".to_string(), 10_000));
    }

    #[test]
    fn chart_periods_follow_the_chart_sizes_and_never_go_finer_than_five_minutes() {
        assert_eq!(period_of(60_000), "5m");
        assert_eq!(period_of(300_000), "5m");
        assert_eq!(period_of(1_800_000), "30m");
        assert_eq!(period_of(7_200_000), "2h");
        assert_eq!(period_of(86_400_000), "1d");
    }
}
