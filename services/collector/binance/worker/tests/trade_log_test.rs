//! Filtering and paging the trades already collected. Pure over a list,
//! so this runs in the default gate — no Postgres and no exchange.

use binance_trades::Trade;
use binance_worker::trade_log::{page, Cursor, TradeQuery, MAX_PAGE};

fn trade(symbol: &str, id: u64, time_ms: u64, is_buyer: bool) -> Trade {
    Trade {
        symbol: symbol.to_string(),
        id,
        order_id: id * 10,
        price: "100".to_string(),
        qty: "1".to_string(),
        commission: "0.1".to_string(),
        commission_asset: "USDT".to_string(),
        time_ms,
        is_buyer,
    }
}

fn query(limit: usize) -> TradeQuery {
    TradeQuery { limit, ..TradeQuery::default() }
}

#[test]
fn orders_by_time_then_symbol_then_id() {
    let trades = vec![
        trade("ETHUSDT", 9, 2_000, true),
        trade("BTCUSDT", 2, 1_000, true),
        trade("BTCUSDT", 1, 1_000, false),
    ];
    let result = page(trades, &query(10));
    let keys: Vec<(String, u64)> = result.trades.iter().map(|t| (t.symbol.clone(), t.id)).collect();
    assert_eq!(
        keys,
        vec![("BTCUSDT".to_string(), 1), ("BTCUSDT".to_string(), 2), ("ETHUSDT".to_string(), 9)]
    );
    assert_eq!(result.next, None);
}

#[test]
fn keeps_only_the_market_asked_for_whatever_the_case() {
    let trades = vec![trade("BTCUSDT", 1, 1_000, true), trade("ETHUSDT", 2, 1_100, true)];
    let result = page(trades, &TradeQuery { symbol: Some("btcusdt".to_string()), ..query(10) });
    assert_eq!(result.trades.len(), 1);
    assert_eq!(result.trades[0].symbol, "BTCUSDT");
}

#[test]
fn bounds_the_period_inclusively_at_both_ends() {
    let trades = vec![
        trade("BTCUSDT", 1, 999, true),
        trade("BTCUSDT", 2, 1_000, true),
        trade("BTCUSDT", 3, 2_000, true),
        trade("BTCUSDT", 4, 2_001, true),
    ];
    let result = page(trades, &TradeQuery { since_ms: Some(1_000), until_ms: Some(2_000), ..query(10) });
    let ids: Vec<u64> = result.trades.iter().map(|t| t.id).collect();
    assert_eq!(ids, vec![2, 3]);
}

#[test]
fn walks_every_trade_exactly_once_across_pages() {
    let all: Vec<Trade> = (1..=7).map(|i| trade("BTCUSDT", i, 1_000 + i, i % 2 == 0)).collect();
    let mut seen: Vec<u64> = Vec::new();
    let mut cursor: Option<Cursor> = None;
    loop {
        let result = page(all.clone(), &TradeQuery { after: cursor.clone(), ..query(3) });
        seen.extend(result.trades.iter().map(|t| t.id));
        match result.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert_eq!(seen, vec![1, 2, 3, 4, 5, 6, 7]);
}

// Two unrelated executions can share a millisecond and a numeric id on
// different markets (`(symbol, id)` is the namespace). A page boundary
// must not drop or repeat either of them.
#[test]
fn separates_the_same_id_and_instant_on_two_markets() {
    let all = vec![trade("BTCUSDT", 5, 1_000, true), trade("ETHUSDT", 5, 1_000, false)];
    let first = page(all.clone(), &query(1));
    assert_eq!(first.trades[0].symbol, "BTCUSDT");
    let next = first.next.expect("a second page");
    let second = page(all, &TradeQuery { after: Some(next), ..query(1) });
    assert_eq!(second.trades.len(), 1);
    assert_eq!(second.trades[0].symbol, "ETHUSDT");
    assert_eq!(second.next, None);
}

#[test]
fn hands_out_no_cursor_when_the_page_is_exactly_the_last_one() {
    let all: Vec<Trade> = (1..=3).map(|i| trade("BTCUSDT", i, 1_000 + i, true)).collect();
    let result = page(all, &query(3));
    assert_eq!(result.trades.len(), 3);
    assert_eq!(result.next, None, "a cursor here would send a caller to an empty page");
}

#[test]
fn a_cursor_survives_a_round_trip_through_text() {
    let cursor = Cursor { time_ms: 1_700_000_000_000, id: 42, symbol: "BTC-USD".to_string() };
    assert_eq!(Cursor::parse(&cursor.encode()), Some(cursor));
}

#[test]
fn refuses_a_cursor_it_did_not_issue() {
    for text in ["", "abc", "1700000000000", "1700000000000:42", "x:42:BTCUSDT", "1:2:"] {
        assert_eq!(Cursor::parse(text), None, "{text} should not parse");
    }
}

#[test]
fn refuses_a_page_size_outside_its_bounds_and_a_period_that_ends_before_it_starts() {
    assert!(query(0).check().is_err());
    assert!(query(MAX_PAGE + 1).check().is_err());
    assert!(query(MAX_PAGE).check().is_ok());
    assert!(TradeQuery { since_ms: Some(2), until_ms: Some(1), ..query(10) }.check().is_err());
    assert!(TradeQuery { since_ms: Some(1), until_ms: Some(1), ..query(10) }.check().is_ok());
}
