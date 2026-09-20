//! A real, credentialed HTTP client against the Binance API — the piece
//! the Binance adapter specification's Gate B0 homologation needs that no
//! other crate in this repo provides (see each Binance crate's own
//! README: "No live HTTP client, no real Binance account was available
//! in this delivery").
//!
//! This client signs every request via [`collector_credentials::signing`]
//! (fixed host, path allowlist — no code path accepts an arbitrary URL),
//! and refuses to do anything else until [`LiveClient::ensure_read_only`]
//! has confirmed the key's `apiRestrictions` are genuinely read-only, per
//! the protocol: "não solicitar poder de mover dinheiro para conseguir
//! consultar histórico." Parsed results feed directly into the existing,
//! already-tested normalizers in `binance-trades`/`binance-catalog`/
//! `binance-flows` — this crate does no normalization of its own beyond
//! mapping the wire JSON onto those crates' `Raw*`/input types.

use binance_catalog::CatalogSnapshot;
use binance_flows::{
    normalize_deposit, normalize_withdrawal, NormalizedFlow, RawDeposit, RawWithdrawal,
};
use binance_trades::Trade;
use collector_credentials::{
    host_for, sign_request, validate_read_only, ApiRestrictions, Environment, RestrictionsError,
    SigningError, ALLOWED_PATHS,
};
use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

/// Parses Binance's `"YYYY-MM-DD HH:MM:SS"` datetime format (used by
/// withdrawal history's `applyTime`/`completeTime`, unlike most other
/// endpoints' plain millisecond timestamps — a real, easy-to-miss
/// inconsistency confirmed against the live API during this delivery)
/// as UTC milliseconds since the epoch.
fn parse_binance_datetime(s: &str) -> Option<u64> {
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()?;
    naive.and_utc().timestamp_millis().try_into().ok()
}

pub use exchange_core::{parallel_map, AccountBalance, RawCandle};

#[derive(Debug, thiserror::Error)]
pub enum LiveClientError {
    #[error("http request failed: {0}")]
    Http(reqwest::Error),
    #[error("response did not decode as expected JSON: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("apiRestrictions rejected: {0}")]
    NotReadOnly(#[from] RestrictionsError),
    #[error("request signing failed: {0}")]
    Signing(#[from] SigningError),
    #[error("read-only check was never performed before this call")]
    ReadOnlyNotConfirmed,
    #[error("binance returned an error: code={code} msg={msg}")]
    ApiError { code: i64, msg: String },
    #[error("flow normalization failed: {0}")]
    Flow(#[from] binance_flows::FlowError),
    #[error("catalog archiving failed: {0}")]
    Catalog(#[from] binance_catalog::CatalogError),
}

/// Cloning is cheap: `reqwest::blocking::Client` is internally
/// `Arc`-backed. This exists so callers can move an already-`ensure_read_only`-confirmed
/// client into a `tokio::task::spawn_blocking` closure (this client's
/// HTTP calls are blocking — see the module doc comment).
#[derive(Clone)]
pub struct LiveClient {
    api_key: String,
    api_secret: String,
    environment: Environment,
    http: reqwest::blocking::Client,
    read_only_confirmed: bool,
}

impl LiveClient {
    pub fn new(
        api_key: impl Into<String>,
        api_secret: impl Into<String>,
        environment: Environment,
    ) -> Self {
        LiveClient {
            api_key: api_key.into(),
            api_secret: api_secret.into(),
            environment,
            http: reqwest::blocking::Client::new(),
            read_only_confirmed: false,
        }
    }

    fn timestamp_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the epoch")
            .as_millis() as u64
    }

    /// Signs and sends a GET request to `path` (which must be in
    /// [`collector_credentials::signing::ALLOWED_PATHS`]) with `params`
    /// plus a fresh `timestamp`. Refuses every path except
    /// `apiRestrictions` until [`Self::ensure_read_only`] has run.
    fn signed_get(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<serde_json::Value, LiveClientError> {
        if !self.read_only_confirmed && path != "/sapi/v1/account/apiRestrictions" {
            return Err(LiveClientError::ReadOnlyNotConfirmed);
        }
        let timestamp = Self::timestamp_ms();
        let mut query = params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        if !query.is_empty() {
            query.push('&');
        }
        query.push_str(&format!("timestamp={timestamp}"));

        let signature = sign_request(&self.api_secret, path, &query)?;
        let url = format!(
            "{}{}?{}&signature={}",
            host_for(self.environment),
            path,
            query,
            signature
        );
        self.get(&url)
    }

    /// A genuinely public endpoint (e.g. `exchangeInfo`): no
    /// `timestamp`/`signature` are sent, since Binance rejects
    /// unrecognized parameters on these paths (`-1104`) rather than
    /// ignoring them. Still gated on [`Self::ensure_read_only`], for the
    /// same reason every call on this client is — a uniform "confirmed
    /// safe, or nothing runs," not a per-endpoint exemption list.
    fn public_get(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<serde_json::Value, LiveClientError> {
        if !self.read_only_confirmed {
            return Err(LiveClientError::ReadOnlyNotConfirmed);
        }
        if !ALLOWED_PATHS.contains(&path) {
            return Err(LiveClientError::Signing(SigningError::PathNotAllowed(
                path.to_string(),
            )));
        }
        let query = params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        let url = if query.is_empty() {
            format!("{}{}", host_for(self.environment), path)
        } else {
            format!("{}{}?{}", host_for(self.environment), path, query)
        };
        self.get(&url)
    }

    fn get(&self, url: &str) -> Result<serde_json::Value, LiveClientError> {
        let response = self
            .http
            .get(url)
            .header("X-MBX-APIKEY", &self.api_key)
            .send()?;
        let body: serde_json::Value = response.json()?;

        if let Some(code) = body.get("code").and_then(|c| c.as_i64()) {
            if body.get("msg").is_some() {
                return Err(LiveClientError::ApiError {
                    code,
                    msg: body
                        .get("msg")
                        .and_then(|m| m.as_str())
                        .unwrap_or_default()
                        .to_string(),
                });
            }
        }
        Ok(body)
    }

    /// Fetches `apiRestrictions` and validates it via
    /// [`collector_credentials::validate_read_only`]. Every other method
    /// on this client refuses to run until this has succeeded — the same
    /// "read-only, or refuse" gate `restrictions.rs` already tests
    /// against synthetic fixtures, now checked against the real key.
    pub fn ensure_read_only(&mut self) -> Result<ApiRestrictions, LiveClientError> {
        let body = self.signed_get("/sapi/v1/account/apiRestrictions", &[])?;
        let restrictions: ApiRestrictions = serde_json::from_value(body)?;
        validate_read_only(&restrictions)?;
        self.read_only_confirmed = true;
        Ok(restrictions)
    }

    /// `GET /api/v3/exchangeInfo`, mapped onto [`CatalogSnapshot`].
    pub fn fetch_exchange_info(&self) -> Result<CatalogSnapshot, LiveClientError> {
        let body = self.public_get("/api/v3/exchangeInfo", &[])?;
        let symbols: BTreeSet<String> = body["symbols"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s["symbol"].as_str().map(str::to_string))
            .collect();
        Ok(CatalogSnapshot {
            captured_at_ms: Self::timestamp_ms(),
            symbols,
        })
    }

    /// `GET /api/v3/exchangeInfo`, mapped onto each symbol's
    /// `(baseAsset, quoteAsset)` — the split `binance-live-client`'s
    /// other methods don't need, but converting a trade fill into
    /// ledger legs does (a `BTCUSDT` fill debits/credits `BTC` and
    /// `USDT`, not an opaque `"BTCUSDT"`).
    pub fn fetch_symbol_assets(
        &self,
    ) -> Result<std::collections::BTreeMap<String, (String, String)>, LiveClientError> {
        let body = self.public_get("/api/v3/exchangeInfo", &[])?;
        let map = body["symbols"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| {
                Some((
                    s["symbol"].as_str()?.to_string(),
                    (
                        s["baseAsset"].as_str()?.to_string(),
                        s["quoteAsset"].as_str()?.to_string(),
                    ),
                ))
            })
            .collect();
        Ok(map)
    }

    /// Every `interval` candle of `symbol` from `start_time_ms` to
    /// `end_time_ms`, paging through Binance's 1000-candle limit — one
    /// request per ~1000 candles for a whole chart's worth of prices,
    /// instead of one per point.
    pub fn fetch_klines_span(
        &self,
        symbol: &str,
        interval: &str,
        start_time_ms: u64,
        end_time_ms: u64,
    ) -> Result<Vec<RawCandle>, LiveClientError> {
        let mut all: Vec<RawCandle> = Vec::new();
        let mut next_start = start_time_ms;
        while next_start <= end_time_ms {
            let page = self.fetch_klines_in_range(symbol, interval, 1000, Some(next_start), Some(end_time_ms))?;
            let Some(last) = page.last() else { break };
            next_start = last.close_time_ms + 1;
            let full_page = page.len() >= 1000;
            all.extend(page);
            if !full_page {
                break;
            }
        }
        Ok(all)
    }

    /// As [`Self::fetch_symbol_assets`], but only for `symbols` — one small
    /// `exchangeInfo?symbol=` request each, run concurrently, instead of
    /// downloading the whole exchange catalog (~17 MB, seconds) to look
    /// up a handful of pairs. A symbol Binance doesn't list (error
    /// `-1121`, "Invalid symbol") is simply absent from the result — the
    /// same "no such market" signal the full catalog gave — while any
    /// other failure is returned, never swallowed.
    pub fn fetch_symbol_assets_for(
        &self,
        symbols: &BTreeSet<String>,
    ) -> Result<std::collections::BTreeMap<String, (String, String)>, LiveClientError> {
        let symbols: Vec<&String> = symbols.iter().collect();
        let lookups = parallel_map(&symbols, |symbol| {
            match self.public_get("/api/v3/exchangeInfo", &[("symbol", (*symbol).clone())]) {
                Ok(body) => Ok(body["symbols"].as_array().and_then(|list| list.first()).and_then(
                    |entry| {
                        Some((
                            entry["symbol"].as_str()?.to_string(),
                            (
                                entry["baseAsset"].as_str()?.to_string(),
                                entry["quoteAsset"].as_str()?.to_string(),
                            ),
                        ))
                    },
                )),
                Err(LiveClientError::ApiError { code: -1121, .. }) => Ok(None),
                Err(other) => Err(other),
            }
        });
        let mut map = std::collections::BTreeMap::new();
        for lookup in lookups {
            if let Some((symbol, assets)) = lookup? {
                map.insert(symbol, assets);
            }
        }
        Ok(map)
    }

    /// `GET /api/v3/myTrades` for one symbol, mapped onto [`Trade`].
    pub fn fetch_my_trades(&self, symbol: &str) -> Result<Vec<Trade>, LiveClientError> {
        let body = self.signed_get(
            "/api/v3/myTrades",
            &[
                ("symbol", symbol.to_string()),
                ("limit", "1000".to_string()),
            ],
        )?;
        let trades = body
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|t| {
                Some(Trade {
                    symbol: t["symbol"].as_str()?.to_string(),
                    id: t["id"].as_u64()?,
                    order_id: t["orderId"].as_u64()?,
                    price: t["price"].as_str()?.to_string(),
                    qty: t["qty"].as_str()?.to_string(),
                    commission: t["commission"].as_str()?.to_string(),
                    commission_asset: t["commissionAsset"].as_str()?.to_string(),
                    time_ms: t["time"].as_u64()?,
                    is_buyer: t["isBuyer"].as_bool()?,
                })
            })
            .collect();
        Ok(trades)
    }

    /// `GET /sapi/v1/capital/deposit/hisrec`, normalized via
    /// [`binance_flows::normalize_deposit`].
    pub fn fetch_deposits(&self) -> Result<Vec<NormalizedFlow>, LiveClientError> {
        let body = self.signed_get("/sapi/v1/capital/deposit/hisrec", &[])?;
        body.as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|d| {
                Some(RawDeposit {
                    tx_id: d["txId"].as_str()?.to_string(),
                    asset: d["coin"].as_str()?.to_string(),
                    amount: d["amount"].as_str()?.to_string(),
                    status: d["status"].as_u64()? as u32,
                    insert_time_ms: d["insertTime"].as_u64()?,
                })
            })
            .map(|raw| normalize_deposit(&raw).map_err(LiveClientError::from))
            .collect()
    }

    /// `GET /sapi/v1/capital/withdraw/history`, normalized via
    /// [`binance_flows::normalize_withdrawal`].
    pub fn fetch_withdrawals(&self) -> Result<Vec<NormalizedFlow>, LiveClientError> {
        let body = self.signed_get("/sapi/v1/capital/withdraw/history", &[])?;
        body.as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|w| {
                Some(RawWithdrawal {
                    id: w["id"].as_str()?.to_string(),
                    asset: w["coin"].as_str()?.to_string(),
                    amount: w["amount"].as_str()?.to_string(),
                    transaction_fee: w["transactionFee"].as_str()?.to_string(),
                    status: w["status"].as_u64()? as u32,
                    apply_time_ms: parse_binance_datetime(w["applyTime"].as_str()?)?,
                })
            })
            .map(|raw| normalize_withdrawal(&raw).map_err(LiveClientError::from))
            .collect()
    }

    /// `GET /api/v3/account`, returning every balance entry Binance
    /// reports (including zero-balance assets — filtering to "held"
    /// assets is the caller's decision, not this method's).
    pub fn fetch_account_balances(&self) -> Result<Vec<AccountBalance>, LiveClientError> {
        let body = self.signed_get("/api/v3/account", &[])?;
        let balances = body["balances"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|b| {
                Some(AccountBalance {
                    asset: b["asset"].as_str()?.to_string(),
                    free: b["free"].as_str()?.to_string(),
                    locked: b["locked"].as_str()?.to_string(),
                })
            })
            .collect();
        Ok(balances)
    }

    /// `GET /api/v3/klines` for `symbol`/`interval`, most recent
    /// `limit` candles (public endpoint — no signature needed, same as
    /// `exchangeInfo`).
    pub fn fetch_klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: u32,
    ) -> Result<Vec<RawCandle>, LiveClientError> {
        self.fetch_klines_in_range(symbol, interval, limit, None, None)
    }

    /// As [`Self::fetch_klines`], but for a historical window
    /// (`start_time_ms`/`end_time_ms`, either or both omitted) rather
    /// than always the most recent candles — what resolving a
    /// *historical* price mark needs.
    pub fn fetch_klines_in_range(
        &self,
        symbol: &str,
        interval: &str,
        limit: u32,
        start_time_ms: Option<u64>,
        end_time_ms: Option<u64>,
    ) -> Result<Vec<RawCandle>, LiveClientError> {
        let mut params = vec![
            ("symbol", symbol.to_string()),
            ("interval", interval.to_string()),
            ("limit", limit.to_string()),
        ];
        if let Some(start) = start_time_ms {
            params.push(("startTime", start.to_string()));
        }
        if let Some(end) = end_time_ms {
            params.push(("endTime", end.to_string()));
        }
        let body = self.public_get("/api/v3/klines", &params)?;
        let candles = body
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let entry = entry.as_array()?;
                Some(RawCandle {
                    open_time_ms: entry.first()?.as_u64()?,
                    close_price: entry.get(4)?.as_str()?.to_string(),
                    close_time_ms: entry.get(6)?.as_u64()?,
                })
            })
            .collect();
        Ok(candles)
    }
}


#[cfg(test)]
mod tests {
    use super::parse_binance_datetime;

    #[test]
    fn parses_binance_withdrawal_datetime_format_as_utc_millis() {
        // "2026-08-19 15:29:17" is a real applyTime this delivery's own
        // homologation run captured from a live withdrawal history
        // response; 1787153357000 is that instant in UTC epoch
        // milliseconds, cross-checked independently via Python's
        // datetime module.
        assert_eq!(
            parse_binance_datetime("2026-08-19 15:29:17"),
            Some(1_787_153_357_000)
        );
    }

    #[test]
    fn rejects_an_unparseable_datetime_instead_of_defaulting_to_zero() {
        assert_eq!(parse_binance_datetime("not-a-date"), None);
    }
}

/// A signed request's URL carries its signature and timestamp; an error must
/// not repeat it into messages shown to people or written to logs.
impl From<reqwest::Error> for LiveClientError {
    fn from(error: reqwest::Error) -> Self {
        LiveClientError::Http(error.without_url())
    }
}

#[cfg(test)]
mod redaction_tests {
    use super::*;

    #[test]
    fn a_failed_request_does_not_repeat_its_signed_url_in_its_error() {
        let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(2)).build().unwrap();
        let error = client.get("http://127.0.0.1:1/api/v3/account?timestamp=1&signature=SECRET-SIGNATURE").send().unwrap_err();
        assert!(error.to_string().contains("SECRET-SIGNATURE"), "premise: reqwest puts the URL in its own message");
        let message = LiveClientError::from(error).to_string();
        assert!(!message.contains("SECRET-SIGNATURE"), "the signature must not appear in: {message}");
    }
}
