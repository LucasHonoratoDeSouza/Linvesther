//! What Kraken's REST API returns, parsed into plain types. Kept apart from
//! the HTTP so every shape can be tested from a fixture.
//!
//! Kraken answers every call with `{"error": [...], "result": {...}}` and an
//! HTTP 200, even when it refuses the call — [`api_errors`] is how a refusal
//! is told apart from an answer.

use exchange_core::RawCandle;
use serde_json::Value;
use std::collections::BTreeMap;

/// The refusal messages in a response (`EGeneral:Permission denied`, ...), if any.
pub fn api_errors(body: &Value) -> Vec<String> {
    body.get("error")
        .and_then(Value::as_array)
        .map(|errors| errors.iter().filter_map(|e| e.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

fn text(value: &Value, key: &str) -> Option<String> {
    match value.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Seconds since the epoch (Kraken sends fractional seconds) as milliseconds.
fn seconds_to_ms(value: &Value) -> Option<u64> {
    let seconds = match value {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => s.parse().ok()?,
        _ => return None,
    };
    if seconds.is_sign_negative() {
        return None;
    }
    Some((seconds * 1000.0).round() as u64)
}

/// A market Kraken lists: the key its responses use, the name it also accepts
/// on requests, and the two assets as Kraken's own codes (`XXBT`, `ZUSD`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairInfo {
    pub key: String,
    pub altname: String,
    pub base: String,
    pub quote: String,
}

pub fn parse_pairs(result: &Value) -> Result<Vec<PairInfo>, String> {
    let pairs = result.as_object().ok_or("the market list was not an object")?;
    let mut out = Vec::with_capacity(pairs.len());
    for (key, pair) in pairs {
        out.push(PairInfo {
            key: key.clone(),
            altname: text(pair, "altname").ok_or("a market had no altname")?,
            base: text(pair, "base").ok_or("a market had no base asset")?,
            quote: text(pair, "quote").ok_or("a market had no quote asset")?,
        });
    }
    Ok(out)
}

/// Kraken's asset code → the short name it shows (`XXBT` → `XBT`, `ZUSD` → `USD`).
pub fn parse_assets(result: &Value) -> Result<BTreeMap<String, String>, String> {
    let assets = result.as_object().ok_or("the asset list was not an object")?;
    Ok(assets.iter().filter_map(|(code, info)| Some((code.clone(), text(info, "altname")?))).collect())
}

/// One asset's balance as Kraken reports it: the total, and how much of it
/// is held by open orders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawBalance {
    pub code: String,
    pub balance: String,
    pub hold: String,
}

pub fn parse_balances(result: &Value) -> Result<Vec<RawBalance>, String> {
    let balances = result.as_object().ok_or("the balance response was not an object")?;
    let mut out = Vec::with_capacity(balances.len());
    for (code, entry) in balances {
        // BalanceEx gives an object per asset; plain Balance gives the number itself.
        let (balance, hold) = match entry {
            Value::String(total) => (total.clone(), "0".to_string()),
            Value::Object(_) => (
                text(entry, "balance").ok_or("a balance had no amount")?,
                text(entry, "hold_trade").unwrap_or_else(|| "0".into()),
            ),
            _ => return Err("a balance was neither an amount nor an object".into()),
        };
        out.push(RawBalance { code: code.clone(), balance, hold });
    }
    Ok(out)
}

/// One executed trade, with the market as Kraken named it in that response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawTrade {
    pub id: String,
    pub order_id: String,
    pub pair: String,
    pub is_buy: bool,
    pub price: String,
    pub volume: String,
    /// Charged in the quote currency.
    pub fee: String,
    pub time_ms: u64,
}

/// A page of trades and the total number Kraken says there are.
pub fn parse_trades_history(result: &Value) -> Result<(Vec<RawTrade>, u64), String> {
    let count = result.get("count").and_then(Value::as_u64).ok_or("the trade history had no count")?;
    let trades = result.get("trades").and_then(Value::as_object).ok_or("the trade history had no trades")?;
    let mut out = Vec::with_capacity(trades.len());
    for (id, trade) in trades {
        out.push(RawTrade {
            id: id.clone(),
            order_id: text(trade, "ordertxid").unwrap_or_default(),
            pair: text(trade, "pair").ok_or("a trade had no pair")?,
            is_buy: match text(trade, "type").as_deref() {
                Some("buy") => true,
                Some("sell") => false,
                other => return Err(format!("a trade had an unknown type {other:?}")),
            },
            price: text(trade, "price").ok_or("a trade had no price")?,
            volume: text(trade, "vol").ok_or("a trade had no volume")?,
            fee: text(trade, "fee").unwrap_or_else(|| "0".into()),
            time_ms: trade.get("time").and_then(seconds_to_ms).ok_or("a trade had no readable time")?,
        });
    }
    Ok((out, count))
}

/// One line of the account ledger: every change to a balance, whatever caused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawLedgerEntry {
    pub id: String,
    /// Groups the lines of one event (a trade's two legs, a conversion's spend and receive).
    pub refid: String,
    pub time_ms: u64,
    /// `trade`, `deposit`, `withdrawal`, `transfer`, `staking`, ...
    pub kind: String,
    pub subtype: String,
    pub asset: String,
    /// Signed: negative when the balance went down.
    pub amount: String,
    pub fee: String,
}

pub fn parse_ledgers(result: &Value) -> Result<(Vec<RawLedgerEntry>, u64), String> {
    let count = result.get("count").and_then(Value::as_u64).ok_or("the ledger had no count")?;
    let ledger = result.get("ledger").and_then(Value::as_object).ok_or("the ledger had no entries")?;
    let mut out = Vec::with_capacity(ledger.len());
    for (id, entry) in ledger {
        out.push(RawLedgerEntry {
            id: id.clone(),
            refid: text(entry, "refid").unwrap_or_default(),
            time_ms: entry.get("time").and_then(seconds_to_ms).ok_or("a ledger entry had no readable time")?,
            kind: text(entry, "type").ok_or("a ledger entry had no type")?,
            subtype: text(entry, "subtype").unwrap_or_default(),
            asset: text(entry, "asset").ok_or("a ledger entry had no asset")?,
            amount: text(entry, "amount").ok_or("a ledger entry had no amount")?,
            fee: text(entry, "fee").unwrap_or_else(|| "0".into()),
        });
    }
    Ok((out, count))
}

/// Candles come oldest-first with the *open* time in seconds, and the last
/// one is the interval still in progress (its close is the live price).
pub fn parse_ohlc(result: &Value, interval_ms: u64) -> Result<Vec<RawCandle>, String> {
    let series = result
        .as_object()
        .and_then(|o| o.iter().find(|(key, _)| key.as_str() != "last"))
        .and_then(|(_, rows)| rows.as_array())
        .ok_or("the candle response had no series")?;
    let mut out = Vec::with_capacity(series.len());
    for row in series {
        let row = row.as_array().ok_or("a candle was not a list")?;
        let open_secs = row.first().and_then(Value::as_u64).ok_or("a candle had no open time")?;
        let close = row.get(4).and_then(Value::as_str).ok_or("a candle had no close")?;
        let open_time_ms = open_secs * 1000;
        out.push(RawCandle { open_time_ms, close_time_ms: open_time_ms + interval_ms - 1, close_price: close.to_string() });
    }
    Ok(out)
}

/// One public trade: its price and when it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicTrade {
    pub price: String,
    pub time_ms: u64,
}

/// A page of the market's public trades, oldest first, and the cursor to ask for the next page.
pub fn parse_public_trades(result: &Value) -> Result<(Vec<PublicTrade>, Option<String>), String> {
    let object = result.as_object().ok_or("the trades response was not an object")?;
    let rows = object
        .iter()
        .find(|(key, _)| key.as_str() != "last")
        .and_then(|(_, rows)| rows.as_array())
        .ok_or("the trades response had no series")?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let row = row.as_array().ok_or("a trade was not a list")?;
        out.push(PublicTrade {
            price: row.first().and_then(Value::as_str).ok_or("a trade had no price")?.to_string(),
            time_ms: row.get(2).and_then(seconds_to_ms).ok_or("a trade had no readable time")?,
        });
    }
    let cursor = object.get("last").and_then(Value::as_str).map(str::to_string);
    Ok((out, cursor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_refusal_is_read_from_the_error_list_even_though_the_status_was_ok() {
        assert_eq!(api_errors(&json!({ "error": ["EGeneral:Permission denied"], "result": {} })), vec!["EGeneral:Permission denied"]);
        assert!(api_errors(&json!({ "error": [], "result": { "a": 1 } })).is_empty());
        assert!(api_errors(&json!({ "result": {} })).is_empty());
    }

    #[test]
    fn markets_and_assets_keep_kraken_own_codes() {
        let pairs = parse_pairs(&json!({
            "XXBTZUSD": { "altname": "XBTUSD", "wsname": "XBT/USD", "base": "XXBT", "quote": "ZUSD" },
            "SOLUSD": { "altname": "SOLUSD", "base": "SOL", "quote": "ZUSD" }
        }))
        .unwrap();
        assert_eq!(pairs.len(), 2);
        let btc = pairs.iter().find(|p| p.key == "XXBTZUSD").unwrap();
        assert_eq!((btc.altname.as_str(), btc.base.as_str(), btc.quote.as_str()), ("XBTUSD", "XXBT", "ZUSD"));
        let assets = parse_assets(&json!({ "XXBT": { "altname": "XBT" }, "SOL": { "altname": "SOL" } })).unwrap();
        assert_eq!(assets["XXBT"], "XBT");
    }

    #[test]
    fn balances_carry_the_total_and_what_open_orders_hold() {
        let ex = parse_balances(&json!({ "XXBT": { "balance": "0.5", "hold_trade": "0.1" }, "ZUSD": { "balance": "12.5" } })).unwrap();
        assert!(ex.contains(&RawBalance { code: "XXBT".into(), balance: "0.5".into(), hold: "0.1".into() }));
        assert!(ex.contains(&RawBalance { code: "ZUSD".into(), balance: "12.5".into(), hold: "0".into() }));
        let plain = parse_balances(&json!({ "XXBT": "0.5" })).unwrap();
        assert_eq!(plain[0].hold, "0");
    }

    #[test]
    fn a_trade_is_read_with_its_side_fee_and_millisecond_time() {
        let result = json!({ "count": 1, "trades": { "TX1": {
            "ordertxid": "O1", "pair": "XXBTZUSD", "time": 1688666559.8974, "type": "sell",
            "ordertype": "limit", "price": "30010.00000", "cost": "300.10000", "fee": "0.48016",
            "vol": "0.01000000", "margin": "0.0", "misc": ""
        }}});
        let (trades, count) = parse_trades_history(&result).unwrap();
        assert_eq!(count, 1);
        let t = &trades[0];
        assert_eq!((t.id.as_str(), t.order_id.as_str(), t.pair.as_str()), ("TX1", "O1", "XXBTZUSD"));
        assert!(!t.is_buy);
        assert_eq!((t.price.as_str(), t.volume.as_str(), t.fee.as_str()), ("30010.00000", "0.01000000", "0.48016"));
        assert_eq!(t.time_ms, 1_688_666_559_897);
        assert!(parse_trades_history(&json!({ "count": 1, "trades": { "T": { "pair": "X", "type": "hold", "price": "1", "vol": "1", "time": 1.0 } } })).is_err());
    }

    #[test]
    fn a_ledger_entry_keeps_its_signed_amount_type_and_group() {
        let result = json!({ "count": 2, "ledger": {
            "L1": { "refid": "R1", "time": 1688464484.1787, "type": "withdrawal", "subtype": "", "aclass": "currency", "asset": "XXBT", "amount": "-0.5000000000", "fee": "0.0005000000", "balance": "1.0" },
            "L2": { "refid": "R2", "time": 1688464000, "type": "deposit", "asset": "ZUSD", "amount": "100.0000", "fee": "0.0000" }
        }});
        let (entries, count) = parse_ledgers(&result).unwrap();
        assert_eq!(count, 2);
        let w = entries.iter().find(|e| e.id == "L1").unwrap();
        assert_eq!((w.kind.as_str(), w.asset.as_str(), w.amount.as_str(), w.fee.as_str()), ("withdrawal", "XXBT", "-0.5000000000", "0.0005000000"));
        assert_eq!(w.time_ms, 1_688_464_484_179);
        assert_eq!(entries.iter().find(|e| e.id == "L2").unwrap().subtype, "");
    }

    #[test]
    fn candles_close_a_millisecond_before_the_next_opens_and_skip_the_cursor_field() {
        let result = json!({ "XXBTZUSD": [
            [1789749000, "1", "2", "0.5", "100.5", "1", "3", 4],
            [1789749060, "1", "2", "0.5", "101", "1", "3", 4]
        ], "last": 1789749000 });
        let candles = parse_ohlc(&result, 60_000).unwrap();
        assert_eq!(candles.iter().map(|c| c.close_price.as_str()).collect::<Vec<_>>(), vec!["100.5", "101"]);
        assert_eq!(candles[0].open_time_ms, 1_789_749_000_000);
        assert_eq!(candles[0].close_time_ms, 1_789_749_059_999);
        assert_eq!(candles[1].open_time_ms, candles[0].close_time_ms + 1);
    }

    #[test]
    fn public_trades_come_with_a_cursor_for_the_next_page() {
        let result = json!({ "XXBTZUSD": [
            ["100.5", "0.1", 1789749000.123, "b", "l", "", 1],
            ["100.6", "0.2", 1789749001.5, "s", "m", "", 2]
        ], "last": "1789749001500000000" });
        let (trades, cursor) = parse_public_trades(&result).unwrap();
        assert_eq!(trades, vec![
            PublicTrade { price: "100.5".into(), time_ms: 1_789_749_000_123 },
            PublicTrade { price: "100.6".into(), time_ms: 1_789_749_001_500 },
        ]);
        assert_eq!(cursor.as_deref(), Some("1789749001500000000"));
    }
}
