//! What Coinbase's Advanced Trade API returns, parsed into plain types.
//! Kept apart from the HTTP so every shape can be tested from a fixture.

use exchange_core::{AccountBalance, RawCandle};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPermissions {
    pub can_view: bool,
    pub can_trade: bool,
    pub can_transfer: bool,
}

/// One executed trade ("fill").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fill {
    pub entry_id: String,
    pub trade_id: String,
    pub order_id: String,
    /// `BTC-USD`.
    pub product_id: String,
    pub is_buy: bool,
    pub price: String,
    /// Filled amount — in base units, unless `size_in_quote`.
    pub size: String,
    /// True when Coinbase reports `size` in quote-currency units.
    pub size_in_quote: bool,
    pub commission: String,
    pub trade_time_ms: u64,
}

fn text(value: &Value, key: &str) -> Option<String> {
    match value.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub fn parse_permissions(body: &Value) -> Result<KeyPermissions, String> {
    Ok(KeyPermissions {
        can_view: body.get("can_view").and_then(Value::as_bool).ok_or("key_permissions had no can_view")?,
        can_trade: flag(body, "can_trade"),
        can_transfer: flag(body, "can_transfer"),
    })
}

/// A page of accounts: `(balances, next cursor)`. Zero balances are kept
/// — an asset the account has fully sold is still part of its history.
pub fn parse_accounts(body: &Value) -> Result<(Vec<AccountBalance>, Option<String>), String> {
    let accounts = body.get("accounts").and_then(Value::as_array).ok_or("accounts response had no accounts list")?;
    let mut balances = Vec::with_capacity(accounts.len());
    for account in accounts {
        let asset = text(account, "currency").ok_or("an account had no currency")?;
        let free = account.get("available_balance").and_then(|b| text(b, "value")).unwrap_or_else(|| "0".into());
        let locked = account.get("hold").and_then(|b| text(b, "value")).unwrap_or_else(|| "0".into());
        balances.push(AccountBalance { asset, free, locked });
    }
    let cursor = if flag(body, "has_next") { text(body, "cursor").filter(|c| !c.is_empty()) } else { None };
    Ok((balances, cursor))
}

fn parse_time_ms(value: &str) -> Option<u64> {
    let parsed = chrono::DateTime::parse_from_rfc3339(value).ok()?;
    u64::try_from(parsed.timestamp_millis()).ok()
}

/// A page of fills: `(fills, next cursor)`.
pub fn parse_fills(body: &Value) -> Result<(Vec<Fill>, Option<String>), String> {
    let fills = body.get("fills").and_then(Value::as_array).ok_or("fills response had no fills list")?;
    let mut out = Vec::with_capacity(fills.len());
    for fill in fills {
        let entry_id = text(fill, "entry_id").ok_or("a fill had no entry_id")?;
        out.push(Fill {
            trade_id: text(fill, "trade_id").unwrap_or_else(|| entry_id.clone()),
            order_id: text(fill, "order_id").unwrap_or_default(),
            product_id: text(fill, "product_id").ok_or("a fill had no product_id")?,
            is_buy: match text(fill, "side").as_deref() {
                Some("BUY") => true,
                Some("SELL") => false,
                other => return Err(format!("a fill had an unknown side {other:?}")),
            },
            price: text(fill, "price").ok_or("a fill had no price")?,
            size: text(fill, "size").ok_or("a fill had no size")?,
            size_in_quote: flag(fill, "size_in_quote"),
            commission: text(fill, "commission").unwrap_or_else(|| "0".into()),
            trade_time_ms: text(fill, "trade_time")
                .as_deref()
                .and_then(parse_time_ms)
                .ok_or("a fill had no readable trade_time")?,
            entry_id,
        });
    }
    let cursor = text(body, "cursor").filter(|c| !c.is_empty());
    Ok((out, cursor))
}

/// Candles come newest-first with second-resolution start times; returned
/// oldest-first as [`RawCandle`]s that close 1 ms before the next opens.
pub fn parse_candles(body: &Value, interval_ms: u64) -> Result<Vec<RawCandle>, String> {
    let candles = body.get("candles").and_then(Value::as_array).ok_or("candles response had no candles list")?;
    let mut out = Vec::with_capacity(candles.len());
    for candle in candles {
        let start_secs: u64 = text(candle, "start").ok_or("a candle had no start")?.parse().map_err(|_| "a candle had an unreadable start")?;
        let open_time_ms = start_secs * 1000;
        out.push(RawCandle {
            open_time_ms,
            close_time_ms: open_time_ms + interval_ms - 1,
            close_price: text(candle, "close").ok_or("a candle had no close")?,
        });
    }
    out.sort_by_key(|c| c.open_time_ms);
    Ok(out)
}

/// `(base, quote)` of a product, or `None` when the body isn't one.
pub fn parse_product(body: &Value) -> Option<(String, String)> {
    Some((text(body, "base_currency_id")?, text(body, "quote_currency_id")?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn permissions_say_what_the_key_may_do() {
        let p = parse_permissions(&json!({ "can_view": true, "can_trade": false, "can_transfer": false, "portfolio_uuid": "x" })).unwrap();
        assert_eq!(p, KeyPermissions { can_view: true, can_trade: false, can_transfer: false });
        assert!(parse_permissions(&json!({ "can_trade": false })).is_err());
    }

    #[test]
    fn accounts_keep_available_and_held_and_follow_the_cursor_only_while_there_is_more() {
        let page = json!({
            "accounts": [
                { "currency": "BTC", "available_balance": { "value": "0.5", "currency": "BTC" }, "hold": { "value": "0.1", "currency": "BTC" } },
                { "currency": "USD", "available_balance": { "value": "120.25", "currency": "USD" }, "hold": { "value": "0", "currency": "USD" } },
            ],
            "has_next": true, "cursor": "abc", "size": 2
        });
        let (balances, cursor) = parse_accounts(&page).unwrap();
        assert_eq!(balances, vec![
            AccountBalance { asset: "BTC".into(), free: "0.5".into(), locked: "0.1".into() },
            AccountBalance { asset: "USD".into(), free: "120.25".into(), locked: "0".into() },
        ]);
        assert_eq!(cursor.as_deref(), Some("abc"));
        let (_, last) = parse_accounts(&json!({ "accounts": [], "has_next": false, "cursor": "abc" })).unwrap();
        assert_eq!(last, None);
    }

    #[test]
    fn a_fill_is_parsed_with_its_side_time_and_fee() {
        let body = json!({ "fills": [{
            "entry_id": "e1", "trade_id": "t1", "order_id": "o1", "product_id": "BTC-USD",
            "side": "SELL", "price": "50000.5", "size": "0.01", "size_in_quote": false,
            "commission": "3.0", "trade_time": "2026-09-18T16:30:00.500Z"
        }], "cursor": "next" });
        let (fills, cursor) = parse_fills(&body).unwrap();
        assert_eq!(fills.len(), 1);
        let f = &fills[0];
        assert!(!f.is_buy && !f.size_in_quote);
        assert_eq!((f.product_id.as_str(), f.price.as_str(), f.size.as_str(), f.commission.as_str()), ("BTC-USD", "50000.5", "0.01", "3.0"));
        assert_eq!(f.trade_time_ms, 1_789_749_000_500);
        assert_eq!(cursor.as_deref(), Some("next"));
        assert!(parse_fills(&json!({ "fills": [{ "entry_id": "e", "product_id": "X", "side": "HOLD", "price": "1", "size": "1", "trade_time": "2026-09-18T16:30:00Z" }] })).is_err());
    }

    #[test]
    fn candles_come_back_oldest_first_and_close_a_millisecond_before_the_next_opens() {
        let body = json!({ "candles": [
            { "start": "1789749060", "close": "101" },
            { "start": "1789749000", "close": "100" },
        ]});
        let candles = parse_candles(&body, 60_000).unwrap();
        assert_eq!(candles.iter().map(|c| c.close_price.as_str()).collect::<Vec<_>>(), vec!["100", "101"]);
        assert_eq!(candles[0].open_time_ms, 1_789_749_000_000);
        assert_eq!(candles[0].close_time_ms, 1_789_749_059_999);
        assert_eq!(candles[1].open_time_ms, candles[0].close_time_ms + 1);
    }

    #[test]
    fn a_product_names_its_base_and_quote() {
        assert_eq!(parse_product(&json!({ "base_currency_id": "SOL", "quote_currency_id": "USD" })), Some(("SOL".into(), "USD".into())));
        assert_eq!(parse_product(&json!({ "error": "NOT_FOUND" })), None);
    }
}
