use crate::auth::{next_nonce, Credentials};
use crate::names::{is_locked_wallet, market_symbol, normalize_asset, split_unlisted_pair};
use crate::wire::{self, PairInfo, RawLedgerEntry};
use exchange_core::{AccountBalance, MarketData, MarketError, RawCandle};
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_BASE_URL: &str = "https://api.kraken.com";
pub const QUOTE_CURRENCY: &str = "USD";
/// Kraken keeps only the latest 720 candles of each size.
const MAX_CANDLES: u64 = 720;
/// The public rate limit is per address; spacing the calls keeps parallel lookups inside it.
const PUBLIC_SPACING: Duration = Duration::from_millis(400);
const RATE_LIMIT_RETRIES: u32 = 3;
/// Kraken refuses a call whose nonce is not larger than the last it saw for the key. Several
/// processes using one key at once can arrive out of order, so a refused call is retried with a
/// newer nonce after a short, uneven pause.
const NONCE_RETRIES: u32 = 8;

#[derive(Debug, thiserror::Error)]
pub enum KrakenError {
    #[error("could not reach Kraken: {0}")]
    Network(String),
    #[error("Kraken refused {path}: {errors}")]
    Api { path: String, errors: String },
    #[error("Kraken sent something unexpected for {path}: {reason}")]
    Unexpected { path: String, reason: String },
    #[error("Kraken did not accept this API key. Check that the key and the secret were copied whole.")]
    BadKey,
    #[error("this API key cannot {permission}, so Kraken refused it ({said}). Edit the key at Kraken and tick that permission, or create a new key.")]
    MissingPermission { permission: &'static str, said: String },
    #[error("this API key can do more than read: {0}. Create a key with only the query permissions.")]
    NotReadOnly(String),
    #[error("could not confirm this key is read-only, so it was not accepted ({0})")]
    Unverifiable(String),
    #[error("Kraken trade history names a market this account cannot resolve: {0}")]
    UnknownMarket(String),
}

impl From<KrakenError> for MarketError {
    fn from(error: KrakenError) -> Self {
        MarketError(error.to_string())
    }
}

/// One executed trade, in Linvesther's names for the market and its assets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trade {
    pub id: String,
    pub order_id: String,
    /// `BTC-USD`.
    pub symbol: String,
    pub base: String,
    pub quote: String,
    pub is_buy: bool,
    pub price: String,
    /// In base units.
    pub volume: String,
    /// Charged in the quote currency.
    pub fee: String,
    pub time_ms: u64,
}

/// One line of the account ledger, with the asset in Linvesther's name for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEntry {
    pub id: String,
    pub refid: String,
    pub time_ms: u64,
    pub kind: String,
    pub subtype: String,
    pub asset: String,
    /// Signed: negative when the balance went down.
    pub amount: String,
    pub fee: String,
}

/// Kraken's asset and market lists, fetched once and used to translate every name.
struct Catalog {
    altnames: BTreeMap<String, String>,
    known: BTreeSet<String>,
    /// `BTC-USD` → the market.
    markets: BTreeMap<String, PairInfo>,
    /// Every name Kraken uses for a market (`XXBTZUSD`, `XBTUSD`) → `BTC-USD`.
    by_name: BTreeMap<String, String>,
}

impl Catalog {
    fn asset(&self, code: &str) -> String {
        normalize_asset(code, &self.altnames, &self.known)
    }
}

/// A read-only Kraken spot connection.
pub struct KrakenClient {
    base_url: String,
    credentials: Credentials,
    http: reqwest::blocking::Client,
    catalog: Mutex<Option<Arc<Catalog>>>,
    next_public_call: Mutex<Instant>,
    /// Base of the wait after a rate-limit answer, and after a refused nonce.
    rate_limit_wait: Duration,
    nonce_wait: Duration,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock is after the epoch").as_millis() as u64
}

/// Percent-encodes a form value: everything but unreserved characters.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn is_denied(errors: &[String]) -> bool {
    errors.iter().any(|e| e.contains("Permission denied"))
}

fn is_invalid_nonce(errors: &[String]) -> bool {
    errors.iter().any(|e| e.contains("Invalid nonce"))
}

fn is_rate_limited(errors: &[String]) -> bool {
    errors.iter().any(|e| e.contains("Rate limit exceeded") || e.contains("Too many requests") || e.contains("Throttled"))
}

/// What the answer to a call that should be refused for a read-only key told us.
enum Probe {
    /// Kraken said the key lacks the permission — exactly what a read-only key gets.
    Denied,
    /// The call went through, or was refused only for what was in it: the key has the permission.
    Allowed,
    /// Kraken said something else, so nothing can be concluded.
    Unclear(String),
}

impl KrakenClient {
    pub fn new(credentials: Credentials) -> Self {
        Self::with_base_url(credentials, DEFAULT_BASE_URL)
    }

    /// The same client talking to another address — a stand-in server in a test.
    pub fn with_base_url(credentials: Credentials, base_url: impl Into<String>) -> Self {
        KrakenClient {
            base_url: base_url.into(),
            credentials,
            http: reqwest::blocking::Client::builder().user_agent("linvesther-collector").build().expect("the HTTP client can be built"),
            catalog: Mutex::new(None),
            next_public_call: Mutex::new(Instant::now()),
            rate_limit_wait: Duration::from_secs(5),
            nonce_wait: Duration::from_millis(150),
        }
    }

    /// Shorter waits, for a test.
    pub fn with_waits(mut self, rate_limit: Duration, nonce: Duration) -> Self {
        self.rate_limit_wait = rate_limit;
        self.nonce_wait = nonce;
        self
    }

    // ---------- transport ----------------------------------------------

    fn call(&self, request: reqwest::blocking::RequestBuilder, path: &str) -> Result<Value, KrakenError> {
        let response = request.send().map_err(|e| KrakenError::Network(e.without_url().to_string()))?;
        let status = response.status();
        let body = response.text().map_err(|e| KrakenError::Network(e.without_url().to_string()))?;
        let parsed: Value = serde_json::from_str(&body).map_err(|_| KrakenError::Unexpected {
            path: path.to_string(),
            reason: format!("answered {status} with something that is not JSON"),
        })?;
        Ok(parsed)
    }

    /// A public call, spaced so several threads together stay inside the rate limit.
    fn public(&self, path: &str, query: &[(&str, String)]) -> Result<Value, KrakenError> {
        for attempt in 0..=RATE_LIMIT_RETRIES {
            let wait = {
                let mut next = self.next_public_call.lock().expect("the spacing lock is not poisoned");
                let slot = (*next).max(Instant::now());
                *next = slot + PUBLIC_SPACING;
                slot.saturating_duration_since(Instant::now())
            };
            std::thread::sleep(wait);
            let body = self.call(self.http.get(format!("{}{path}", self.base_url)).query(query), path)?;
            let errors = wire::api_errors(&body);
            if errors.is_empty() {
                return body.get("result").cloned().ok_or_else(|| KrakenError::Unexpected { path: path.to_string(), reason: "no result".into() });
            }
            if is_rate_limited(&errors) && attempt < RATE_LIMIT_RETRIES {
                std::thread::sleep(Duration::from_secs(2 * (attempt as u64 + 1)));
                continue;
            }
            return Err(KrakenError::Api { path: path.to_string(), errors: errors.join(", ") });
        }
        unreachable!("the loop returns on its last attempt")
    }

    /// A signed private call. Returns the errors alongside the body so the
    /// permission probes can read a refusal as an answer.
    fn private_raw(&self, path: &str, params: &[(&str, String)]) -> Result<(Vec<String>, Value), KrakenError> {
        let mut rate_limited = 0;
        let mut nonce_refused = 0;
        loop {
            let nonce = next_nonce().to_string();
            let mut post = format!("nonce={nonce}");
            for (key, value) in params {
                post.push('&');
                post.push_str(key);
                post.push('=');
                post.push_str(&encode(value));
            }
            let request = self
                .http
                .post(format!("{}{path}", self.base_url))
                .header("API-Key", self.credentials.api_key())
                .header("API-Sign", self.credentials.sign(path, &nonce, &post))
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(post);
            let body = self.call(request, path)?;
            let errors = wire::api_errors(&body);
            if is_rate_limited(&errors) && rate_limited < RATE_LIMIT_RETRIES {
                rate_limited += 1;
                std::thread::sleep(self.rate_limit_wait * rate_limited);
                continue;
            }
            if is_invalid_nonce(&errors) && nonce_refused < NONCE_RETRIES {
                nonce_refused += 1;
                // The wait grows with each refusal and differs from one process to the next.
                let jitter = Duration::from_nanos(next_nonce() % 1_000 * self.nonce_wait.as_nanos() as u64 / 1_000);
                std::thread::sleep(self.nonce_wait * nonce_refused + jitter);
                continue;
            }
            return Ok((errors, body));
        }
    }

    fn private(&self, path: &str, params: &[(&str, String)]) -> Result<Value, KrakenError> {
        let (errors, body) = self.private_raw(path, params)?;
        if errors.is_empty() {
            return body.get("result").cloned().ok_or_else(|| KrakenError::Unexpected { path: path.to_string(), reason: "no result".into() });
        }
        if errors.iter().any(|e| e.contains("Invalid key") || e.contains("Invalid signature")) {
            return Err(KrakenError::BadKey);
        }
        Err(KrakenError::Api { path: path.to_string(), errors: errors.join(", ") })
    }

    fn unexpected(path: &str, reason: String) -> KrakenError {
        KrakenError::Unexpected { path: path.to_string(), reason }
    }

    // ---------- names ---------------------------------------------------

    fn catalog(&self) -> Result<Arc<Catalog>, KrakenError> {
        let mut held = self.catalog.lock().expect("the catalog lock is not poisoned");
        if let Some(catalog) = held.as_ref() {
            return Ok(catalog.clone());
        }
        let assets = wire::parse_assets(&self.public("/0/public/Assets", &[])?).map_err(|r| Self::unexpected("/0/public/Assets", r))?;
        let pairs = wire::parse_pairs(&self.public("/0/public/AssetPairs", &[])?).map_err(|r| Self::unexpected("/0/public/AssetPairs", r))?;
        let known: BTreeSet<String> = assets.values().cloned().collect();
        let mut catalog = Catalog { altnames: assets, known, markets: BTreeMap::new(), by_name: BTreeMap::new() };
        for pair in pairs {
            let symbol = market_symbol(&catalog.asset(&pair.base), &catalog.asset(&pair.quote));
            catalog.by_name.insert(pair.key.clone(), symbol.clone());
            catalog.by_name.insert(pair.altname.clone(), symbol.clone());
            catalog.markets.entry(symbol).or_insert(pair);
        }
        let catalog = Arc::new(catalog);
        *held = Some(catalog.clone());
        Ok(catalog)
    }

    /// `(symbol, base, quote)` for a market as Kraken named it in a response.
    fn resolve_market(&self, catalog: &Catalog, name: &str) -> Result<(String, String, String), KrakenError> {
        if let Some(symbol) = catalog.by_name.get(name) {
            let (base, quote) = symbol.split_once('-').expect("symbols are BASE-QUOTE");
            return Ok((symbol.clone(), base.to_string(), quote.to_string()));
        }
        let (base, quote) = split_unlisted_pair(name).ok_or_else(|| KrakenError::UnknownMarket(name.to_string()))?;
        let (base, quote) = (catalog.asset(&base), catalog.asset(&quote));
        Ok((market_symbol(&base, &quote), base, quote))
    }

    // ---------- account ---------------------------------------------------

    /// Refuses any key that can trade or move money, and any that cannot read what
    /// Linvesther needs — the same "read-only, or nothing runs" rule the other
    /// exchanges enforce. Done when a key is first connected.
    pub fn ensure_read_only(&self) -> Result<(), KrakenError> {
        self.ensure_can_read()?;
        self.ensure_cannot_write()
    }

    /// The key can read balances, trades and the ledger. Anything missing is
    /// named, so the person knows which box to tick.
    pub fn ensure_can_read(&self) -> Result<(), KrakenError> {
        // Where each one sits in Kraken's key form is part of the message: the ledger
        // permission is under "Data", away from the funds and orders ones, and is easy to miss.
        self.require("“Query funds” (under Funds)", "/0/private/BalanceEx", &[])?;
        self.require("query trades: tick “Query open orders & trades” and “Query closed orders & trades” (under Orders and trades)", "/0/private/TradesHistory", &[("ofs", "0".into())])?;
        self.require("“Query ledger entries” (under Data)", "/0/private/Ledgers", &[("ofs", "0".into())])
    }

    /// The key cannot place orders or withdraw. Kraken has no way to ask a key what
    /// it may do, so each dangerous permission is tried with a call that changes
    /// nothing, and a read-only key is the one Kraken refuses. Cheap enough to
    /// repeat now and then, in case the key's permissions were widened later.
    pub fn ensure_cannot_write(&self) -> Result<(), KrakenError> {
        // Placing an order, checked but never sent (`validate`).
        let order = [
            ("pair", "XBTUSD".to_string()),
            ("type", "buy".to_string()),
            ("ordertype", "limit".to_string()),
            ("price", "1".to_string()),
            ("volume", "0.0001".to_string()),
            ("validate", "true".to_string()),
        ];
        match self.probe("/0/private/AddOrder", &order, "EOrder:")? {
            Probe::Denied => {}
            Probe::Allowed => return Err(KrakenError::NotReadOnly("it can place orders".into())),
            Probe::Unclear(said) => return Err(KrakenError::Unverifiable(said)),
        }
        // Listing the withdrawal methods needs both the query and the withdrawal permission
        // and only lists. (`WithdrawStatus` will not do: Kraken also allows it with
        // "Query ledger entries", which this key must have.) If Kraken no longer answers
        // that call, the equally harmless fee lookup for a withdrawal is tried instead.
        let verdict = match self.probe("/0/private/WithdrawMethods", &[], "EFunding:")? {
            Probe::Unclear(_) => {
                let quote = [("asset", "XBT".to_string()), ("key", "linvesther-check".to_string()), ("amount", "0.001".to_string())];
                self.probe("/0/private/WithdrawInfo", &quote, "EFunding:")?
            }
            answered => answered,
        };
        match verdict {
            Probe::Denied => Ok(()),
            Probe::Allowed => Err(KrakenError::NotReadOnly("it can withdraw funds".into())),
            Probe::Unclear(said) => Err(KrakenError::Unverifiable(said)),
        }
    }

    fn require(&self, permission: &'static str, path: &str, params: &[(&str, String)]) -> Result<(), KrakenError> {
        let (errors, _) = self.private_raw(path, params)?;
        if errors.is_empty() {
            return Ok(());
        }
        if is_denied(&errors) {
            return Err(KrakenError::MissingPermission { permission, said: errors.join(", ") });
        }
        if errors.iter().any(|e| e.contains("Invalid key") || e.contains("Invalid signature")) {
            return Err(KrakenError::BadKey);
        }
        Err(KrakenError::Api { path: path.to_string(), errors: errors.join(", ") })
    }

    /// `own_prefix` marks the refusals that come from what was asked for, after
    /// the permission was already accepted (`EOrder:...` for an order).
    fn probe(&self, path: &str, params: &[(&str, String)], own_prefix: &str) -> Result<Probe, KrakenError> {
        let (errors, _) = self.private_raw(path, params)?;
        if errors.is_empty() {
            return Ok(Probe::Allowed);
        }
        if is_denied(&errors) {
            return Ok(Probe::Denied);
        }
        if errors.iter().all(|e| e.starts_with(own_prefix)) {
            return Ok(Probe::Allowed);
        }
        Ok(Probe::Unclear(errors.join(", ")))
    }

    pub fn balances(&self) -> Result<Vec<AccountBalance>, KrakenError> {
        let path = "/0/private/BalanceEx";
        let raw = wire::parse_balances(&self.private(path, &[])?).map_err(|r| Self::unexpected(path, r))?;
        let catalog = self.catalog()?;
        let mut merged: BTreeMap<String, (Decimal, Decimal)> = BTreeMap::new();
        for balance in raw {
            let total = Decimal::from_str(&balance.balance).map_err(|_| Self::unexpected(path, format!("{} had an unreadable balance", balance.code)))?;
            let hold = Decimal::from_str(&balance.hold).unwrap_or_default();
            let (free, locked) = if is_locked_wallet(&balance.code, &catalog.altnames) { (Decimal::ZERO, total) } else { (total - hold, hold) };
            let entry = merged.entry(catalog.asset(&balance.code)).or_default();
            entry.0 += free;
            entry.1 += locked;
        }
        Ok(merged.into_iter().map(|(asset, (free, locked))| AccountBalance { asset, free: free.to_string(), locked: locked.to_string() }).collect())
    }

    /// Every trade after `since_ms`, oldest first. (Kraken's `start` is exclusive
    /// and in whole seconds, so it is taken one second early; a repeat is harmless.)
    pub fn trades_since(&self, since_ms: u64) -> Result<Vec<Trade>, KrakenError> {
        let path = "/0/private/TradesHistory";
        let catalog = self.catalog()?;
        let start = (since_ms / 1000).saturating_sub(1).to_string();
        let mut out = Vec::new();
        let mut offset = 0u64;
        loop {
            let result = self.private(path, &[("type", "all".into()), ("trades", "false".into()), ("start", start.clone()), ("ofs", offset.to_string())])?;
            let (page, total) = wire::parse_trades_history(&result).map_err(|r| Self::unexpected(path, r))?;
            let fetched = page.len() as u64;
            for trade in page {
                let (symbol, base, quote) = self.resolve_market(&catalog, &trade.pair)?;
                out.push(Trade { id: trade.id, order_id: trade.order_id, symbol, base, quote, is_buy: trade.is_buy, price: trade.price, volume: trade.volume, fee: trade.fee, time_ms: trade.time_ms });
            }
            offset += fetched;
            if fetched == 0 || offset >= total {
                break;
            }
        }
        out.sort_by(|a, b| (a.time_ms, &a.id).cmp(&(b.time_ms, &b.id)));
        Ok(out)
    }

    /// Every ledger line after `since_ms`, oldest first.
    pub fn ledger_since(&self, since_ms: u64) -> Result<Vec<LedgerEntry>, KrakenError> {
        let path = "/0/private/Ledgers";
        let catalog = self.catalog()?;
        let start = (since_ms / 1000).saturating_sub(1).to_string();
        let mut out: Vec<RawLedgerEntry> = Vec::new();
        let mut offset = 0u64;
        loop {
            let result = self.private(path, &[("start", start.clone()), ("ofs", offset.to_string())])?;
            let (page, total) = wire::parse_ledgers(&result).map_err(|r| Self::unexpected(path, r))?;
            let fetched = page.len() as u64;
            out.extend(page);
            offset += fetched;
            if fetched == 0 || offset >= total {
                break;
            }
        }
        let mut entries: Vec<LedgerEntry> = out
            .into_iter()
            .map(|e| LedgerEntry { id: e.id, refid: e.refid, time_ms: e.time_ms, kind: e.kind, subtype: e.subtype, asset: catalog.asset(&e.asset), amount: e.amount, fee: e.fee })
            .collect();
        entries.sort_by(|a, b| (a.time_ms, &a.id).cmp(&(b.time_ms, &b.id)));
        Ok(entries)
    }

    // ---------- prices ------------------------------------------------------

    fn market(&self, symbol: &str) -> Result<PairInfo, KrakenError> {
        self.catalog()?.markets.get(symbol).cloned().ok_or_else(|| KrakenError::UnknownMarket(symbol.to_string()))
    }

    /// Candles of Kraken's own size (`minutes`), oldest first, the still-open one last.
    fn ohlc(&self, pair: &str, minutes: u64, since_secs: u64) -> Result<Vec<RawCandle>, KrakenError> {
        let path = "/0/public/OHLC";
        let result = self.public(path, &[("pair", pair.to_string()), ("interval", minutes.to_string()), ("since", since_secs.to_string())])?;
        wire::parse_ohlc(&result, minutes * 60_000).map_err(|r| Self::unexpected(path, r))
    }

    /// The price of the last public trade at or before `at_ms`, for moments too old
    /// for the one-minute candles Kraken still keeps (the latest twelve hours).
    fn price_from_trades(&self, pair: &str, at_ms: u64) -> Result<Option<Decimal>, KrakenError> {
        let path = "/0/public/Trades";
        let mut window_ms = 60_000u64;
        while window_ms <= 14 * 24 * 3_600_000 {
            let mut cursor = (at_ms.saturating_sub(window_ms) as u128 * 1_000_000).to_string();
            let mut price: Option<String> = None;
            loop {
                let result = self.public(path, &[("pair", pair.to_string()), ("since", cursor.clone()), ("count", "1000".into())])?;
                let (trades, next) = wire::parse_public_trades(&result).map_err(|r| Self::unexpected(path, r))?;
                let page_len = trades.len();
                let mut passed = false;
                for trade in trades {
                    if trade.time_ms <= at_ms {
                        price = Some(trade.price);
                    } else {
                        passed = true;
                        break;
                    }
                }
                match next {
                    Some(next) if !passed && page_len >= 1000 && next != cursor => cursor = next,
                    _ => break,
                }
            }
            if let Some(price) = price {
                return Ok(Decimal::from_str(&price).ok());
            }
            window_ms *= 4;
        }
        Ok(None)
    }
}

/// Kraken's candle size for one of the sizes a chart may use, and how many of
/// its candles make up the requested one.
fn native_interval(interval_ms: u64) -> Result<(u64, u64), MarketError> {
    Ok(match interval_ms {
        60_000 => (1, 1),
        300_000 => (5, 1),
        900_000 => (15, 1),
        1_800_000 => (30, 1),
        3_600_000 => (60, 1),
        // Kraken has no two-hour candle: two of its hourly ones make one.
        7_200_000 => (60, 2),
        86_400_000 => (1440, 1),
        other => return Err(MarketError(format!("no Kraken candle size of {other} ms"))),
    })
}

/// Joins runs of `group` candles into one: opens with the first, closes with the last.
fn aggregate(candles: Vec<RawCandle>, interval_ms: u64) -> Vec<RawCandle> {
    let mut out: Vec<RawCandle> = Vec::new();
    for candle in candles {
        let bucket = candle.open_time_ms - candle.open_time_ms % interval_ms;
        match out.last_mut() {
            Some(last) if last.open_time_ms == bucket => last.close_price = candle.close_price,
            _ => out.push(RawCandle { open_time_ms: bucket, close_time_ms: bucket + interval_ms - 1, close_price: candle.close_price }),
        }
    }
    out
}

impl MarketData for KrakenClient {
    fn quote_currency(&self) -> &str {
        QUOTE_CURRENCY
    }

    fn market_symbol(&self, asset: &str) -> String {
        market_symbol(asset, QUOTE_CURRENCY)
    }

    fn fetch_symbol_assets_for(&self, symbols: &BTreeSet<String>) -> Result<BTreeMap<String, (String, String)>, MarketError> {
        let catalog = self.catalog()?;
        let mut map = BTreeMap::new();
        for symbol in symbols {
            // Only markets Kraken lists: "no such market" has to stay distinguishable from
            // a failure, since it is how an asset with no dollar price is told apart.
            if let Some(pair) = catalog.markets.get(symbol) {
                map.insert(symbol.clone(), (catalog.asset(&pair.base), catalog.asset(&pair.quote)));
            }
        }
        Ok(map)
    }

    fn fetch_account_balances(&self) -> Result<Vec<AccountBalance>, MarketError> {
        Ok(self.balances()?)
    }

    fn fetch_candles_span(&self, symbol: &str, interval_ms: u64, start_ms: u64, end_ms: u64) -> Result<Vec<RawCandle>, MarketError> {
        let (minutes, group) = native_interval(interval_ms)?;
        let pair = self.market(symbol)?;
        let raw = self.ohlc(&pair.key, minutes, (start_ms / 1000).saturating_sub(1))?;
        let mut candles = if group > 1 { aggregate(raw, interval_ms) } else { raw };
        candles.retain(|c| c.open_time_ms <= end_ms);
        // Kraken only serves the latest 720 candles of a size; if what came back
        // starts after the range does, the earlier part is simply not there.
        if let Some(first) = candles.first() {
            if first.open_time_ms > start_ms + 2 * interval_ms {
                return Err(MarketError(format!(
                    "Kraken only keeps the latest {MAX_CANDLES} candles of {minutes} minutes for {symbol}, and the range asked for starts before them"
                )));
            }
        }
        Ok(candles)
    }

    /// The close of the last one-minute candle that had closed by `at_ms`.
    fn price_at_instant(&self, asset: &str, at_ms: u64) -> Result<Decimal, MarketError> {
        let symbol = self.market_symbol(asset);
        let pair = self.market(&symbol)?;
        // One-minute candles reach back twelve hours; leave a margin.
        if at_ms + 11 * 3_600_000 >= now_ms() {
            let candles = self.ohlc(&pair.key, 1, (at_ms / 1000).saturating_sub(6 * 60 + 1))?;
            if let Some(candle) = candles.iter().rev().find(|c| c.close_time_ms <= at_ms) {
                return Decimal::from_str(&candle.close_price).map_err(|_| MarketError(format!("{symbol}: invalid price {}", candle.close_price)));
            }
        }
        self.price_from_trades(&pair.key, at_ms)?.ok_or_else(|| MarketError(format!("{symbol}: no trade found by {at_ms}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(open_min: u64, close: &str) -> RawCandle {
        RawCandle { open_time_ms: open_min * 60_000, close_time_ms: open_min * 60_000 + 3_599_999, close_price: close.into() }
    }

    #[test]
    fn form_values_are_percent_encoded_except_unreserved_characters() {
        assert_eq!(encode("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(encode("1,2 3&4"), "1%2C2%203%264");
    }

    #[test]
    fn two_hourly_candles_make_one_two_hour_candle_that_closes_with_the_second() {
        let hourly = vec![candle(0, "1"), candle(60, "2"), candle(120, "3"), candle(180, "4"), candle(240, "5")];
        let two_hour = aggregate(hourly, 7_200_000);
        assert_eq!(two_hour.iter().map(|c| c.close_price.as_str()).collect::<Vec<_>>(), vec!["2", "4", "5"]);
        assert_eq!(two_hour[0].open_time_ms, 0);
        assert_eq!(two_hour[0].close_time_ms, 7_199_999);
        assert_eq!(two_hour[1].open_time_ms, 7_200_000);
    }

    #[test]
    fn every_chart_step_maps_to_a_candle_size_kraken_serves() {
        for step in exchange_core::SERIES_STEPS_MS {
            assert!(native_interval(step).is_ok(), "no Kraken candle for {step} ms");
        }
        assert_eq!(native_interval(7_200_000).unwrap(), (60, 2));
        assert!(native_interval(123).is_err());
    }

    #[test]
    fn only_a_permission_refusal_counts_as_denied() {
        assert!(is_denied(&["EGeneral:Permission denied".to_string()]));
        assert!(!is_denied(&["EOrder:Insufficient funds".to_string()]));
        assert!(is_rate_limited(&["EAPI:Rate limit exceeded".to_string()]));
        assert!(!is_rate_limited(&["EGeneral:Permission denied".to_string()]));
        assert!(is_invalid_nonce(&["EAPI:Invalid nonce".to_string()]) && !is_invalid_nonce(&["EAPI:Rate limit exceeded".to_string()]));
    }
}
