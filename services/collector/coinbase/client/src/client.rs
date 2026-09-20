use crate::auth::{random_nonce, Credentials};
use crate::wire::{self, Fill, KeyPermissions};
use exchange_core::{parallel_map, AccountBalance, MarketData, MarketError, RawCandle};
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

const HOST: &str = "api.coinbase.com";
const BASE_URL: &str = "https://api.coinbase.com";
/// Coinbase returns at most this many candles per request.
const MAX_CANDLES_PER_REQUEST: u64 = 350;
pub const QUOTE_CURRENCY: &str = "USD";

#[derive(Debug, thiserror::Error)]
pub enum CoinbaseError {
    #[error("could not reach Coinbase: {0}")]
    Network(String),
    /// `not_found` is true for a 404 — how a market that doesn't exist looks.
    #[error("Coinbase answered {status} for {path}: {message}")]
    Api { status: u16, path: String, message: String, not_found: bool },
    #[error("Coinbase sent something unexpected for {path}: {reason}")]
    Unexpected { path: String, reason: String },
    #[error("this API key can do more than read: {0}. Create a key with only the \"View\" permission.")]
    NotReadOnly(String),
}

impl From<CoinbaseError> for MarketError {
    fn from(error: CoinbaseError) -> Self {
        MarketError(error.to_string())
    }
}

/// A read-only Coinbase Advanced Trade connection.
pub struct CoinbaseClient {
    credentials: Credentials,
    http: reqwest::blocking::Client,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock is after the epoch").as_secs()
}

/// Coinbase's name for a candle size.
fn granularity(interval_ms: u64) -> Result<&'static str, MarketError> {
    Ok(match interval_ms {
        60_000 => "ONE_MINUTE",
        300_000 => "FIVE_MINUTE",
        900_000 => "FIFTEEN_MINUTE",
        1_800_000 => "THIRTY_MINUTE",
        3_600_000 => "ONE_HOUR",
        7_200_000 => "TWO_HOUR",
        86_400_000 => "ONE_DAY",
        other => return Err(MarketError(format!("no Coinbase candle size of {other} ms"))),
    })
}

impl CoinbaseClient {
    pub fn new(credentials: Credentials) -> Self {
        CoinbaseClient { credentials, http: reqwest::blocking::Client::new() }
    }

    fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, CoinbaseError> {
        let token = self.credentials.jwt("GET", HOST, path, now_secs(), &random_nonce());
        let response = self
            .http
            .get(format!("{BASE_URL}{path}"))
            .query(query)
            .bearer_auth(token)
            .send()
            .map_err(|e| CoinbaseError::Network(e.without_url().to_string()))?;
        let status = response.status();
        let body = response.text().map_err(|e| CoinbaseError::Network(e.without_url().to_string()))?;
        if !status.is_success() {
            let message = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v.get("message").or_else(|| v.get("error")).and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| body.chars().take(300).collect());
            return Err(CoinbaseError::Api { status: status.as_u16(), path: path.to_string(), message, not_found: status.as_u16() == 404 });
        }
        serde_json::from_str(&body).map_err(|e| CoinbaseError::Unexpected { path: path.to_string(), reason: format!("not JSON: {e}") })
    }

    fn unexpected(path: &str, reason: String) -> CoinbaseError {
        CoinbaseError::Unexpected { path: path.to_string(), reason }
    }

    pub fn key_permissions(&self) -> Result<KeyPermissions, CoinbaseError> {
        let path = "/api/v3/brokerage/key_permissions";
        wire::parse_permissions(&self.get(path, &[])?).map_err(|r| Self::unexpected(path, r))
    }

    /// Refuses any key that can trade or move money — the same "read-only,
    /// or nothing runs" rule the Binance connection enforces.
    pub fn ensure_read_only(&self) -> Result<(), CoinbaseError> {
        let permissions = self.key_permissions()?;
        if !permissions.can_view {
            return Err(CoinbaseError::NotReadOnly("it cannot even view the account".into()));
        }
        let mut extra = Vec::new();
        if permissions.can_trade {
            extra.push("it can trade");
        }
        if permissions.can_transfer {
            extra.push("it can transfer funds");
        }
        if extra.is_empty() {
            Ok(())
        } else {
            Err(CoinbaseError::NotReadOnly(extra.join(" and ")))
        }
    }

    pub fn balances(&self) -> Result<Vec<AccountBalance>, CoinbaseError> {
        let path = "/api/v3/brokerage/accounts";
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut query = vec![("limit", "250".to_string())];
            if let Some(c) = &cursor {
                query.push(("cursor", c.clone()));
            }
            let (page, next) = wire::parse_accounts(&self.get(path, &query)?).map_err(|r| Self::unexpected(path, r))?;
            all.extend(page);
            match next {
                Some(c) => cursor = Some(c),
                None => return Ok(all),
            }
        }
    }

    /// `(base, quote)` for `product_id`, or `None` when Coinbase has no such market.
    pub fn product(&self, product_id: &str) -> Result<Option<(String, String)>, CoinbaseError> {
        let path = format!("/api/v3/brokerage/products/{product_id}");
        match self.get(&path, &[]) {
            Ok(body) => Ok(wire::parse_product(&body)),
            Err(CoinbaseError::Api { not_found: true, .. }) => Ok(None),
            Err(other) => Err(other),
        }
    }

    /// Every fill since `since_ms`, oldest first.
    pub fn fills_since(&self, since_ms: u64) -> Result<Vec<Fill>, CoinbaseError> {
        let path = "/api/v3/brokerage/orders/historical/fills";
        let start = chrono::DateTime::from_timestamp_millis(since_ms as i64)
            .ok_or_else(|| Self::unexpected(path, "the start time is out of range".into()))?
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut query = vec![("start_sequence_timestamp", start.clone()), ("limit", "100".to_string())];
            if let Some(c) = &cursor {
                query.push(("cursor", c.clone()));
            }
            let (page, next) = wire::parse_fills(&self.get(path, &query)?).map_err(|r| Self::unexpected(path, r))?;
            let empty = page.is_empty();
            all.extend(page);
            match next {
                Some(c) if !empty => cursor = Some(c),
                _ => break,
            }
        }
        all.sort_by_key(|f| f.trade_time_ms);
        Ok(all)
    }

    fn candles(&self, product_id: &str, interval_ms: u64, start_ms: u64, end_ms: u64) -> Result<Vec<RawCandle>, CoinbaseError> {
        let path = format!("/api/v3/brokerage/products/{product_id}/candles");
        let granularity = granularity(interval_ms).map_err(|e| Self::unexpected(&path, e.0))?;
        let body = self.get(
            &path,
            &[
                ("start", (start_ms / 1000).to_string()),
                ("end", end_ms.div_ceil(1000).to_string()),
                ("granularity", granularity.to_string()),
            ],
        )?;
        wire::parse_candles(&body, interval_ms).map_err(|r| Self::unexpected(&path, r))
    }
}

impl MarketData for CoinbaseClient {
    fn quote_currency(&self) -> &str {
        QUOTE_CURRENCY
    }

    fn market_symbol(&self, asset: &str) -> String {
        format!("{asset}-{QUOTE_CURRENCY}")
    }

    fn fetch_symbol_assets_for(&self, symbols: &BTreeSet<String>) -> Result<BTreeMap<String, (String, String)>, MarketError> {
        let symbols: Vec<&String> = symbols.iter().collect();
        let mut map = BTreeMap::new();
        for (symbol, found) in symbols.iter().zip(parallel_map(&symbols, |s| self.product(s))) {
            if let Some(assets) = found? {
                map.insert((*symbol).clone(), assets);
            }
        }
        Ok(map)
    }

    fn fetch_account_balances(&self) -> Result<Vec<AccountBalance>, MarketError> {
        Ok(self.balances()?)
    }

    fn fetch_candles_span(&self, symbol: &str, interval_ms: u64, start_ms: u64, end_ms: u64) -> Result<Vec<RawCandle>, MarketError> {
        let window = MAX_CANDLES_PER_REQUEST * interval_ms;
        let mut windows = Vec::new();
        let mut from = start_ms - start_ms % interval_ms;
        while from <= end_ms {
            windows.push((from, (from + window).min(end_ms)));
            from += window;
        }
        let mut all: Vec<RawCandle> = Vec::new();
        for page in parallel_map(&windows, |(from, to)| self.candles(symbol, interval_ms, *from, *to)) {
            all.extend(page?);
        }
        all.sort_by_key(|c| c.open_time_ms);
        all.dedup_by_key(|c| c.open_time_ms);
        Ok(all)
    }

    /// The close of the last one-minute candle that had closed by `at_ms`.
    fn price_at_instant(&self, asset: &str, at_ms: u64) -> Result<Decimal, MarketError> {
        let symbol = self.market_symbol(asset);
        let candles = self.candles(&symbol, 60_000, at_ms.saturating_sub(6 * 60_000), at_ms)?;
        let candle = candles
            .iter()
            .rev()
            .find(|c| c.close_time_ms <= at_ms)
            .ok_or_else(|| MarketError(format!("{symbol}: no closed one-minute candle by {at_ms}")))?;
        Decimal::from_str(&candle.close_price).map_err(|_| MarketError(format!("{symbol}: invalid price {}", candle.close_price)))
    }
}
