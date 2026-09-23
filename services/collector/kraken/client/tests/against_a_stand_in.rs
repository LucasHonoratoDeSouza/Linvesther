//! The client against a stand-in for Kraken on a local port: every call it makes
//! is a real HTTP request, signed for real, answered with what Kraken sends.

use exchange_core::MarketData;
use kraken_client::{Credentials, KrakenClient, KrakenError};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

const SECRET: &str = "kQH5HW/8p1uGOVjbgWA7FunAmGO8lsSUXNsu3eow76sz84Q18fWxnyRzBHCd3pd5nE9qa99HAZtuZuj6F1huXg==";

#[derive(Clone, Debug)]
struct Seen {
    path: String,
    body: String,
    api_key: Option<String>,
    api_sign: Option<String>,
}

type Handler = dyn Fn(&str, &str) -> String + Send + Sync;

/// Serves canned answers until dropped; remembers every request.
struct StandIn {
    url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

fn stand_in(handler: impl Fn(&str, &str) -> String + Send + Sync + 'static) -> StandIn {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let handler: Arc<Handler> = Arc::new(handler);
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let (handler, log) = (handler.clone(), log.clone());
            std::thread::spawn(move || {
                let mut raw = Vec::new();
                let mut chunk = [0u8; 4096];
                let (head, mut body) = loop {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    raw.extend_from_slice(&chunk[..n]);
                    if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        break (String::from_utf8_lossy(&raw[..at]).to_string(), raw[at + 4..].to_vec());
                    }
                };
                let header = |name: &str| head.lines().find_map(|l| l.to_lowercase().strip_prefix(&format!("{name}:")).map(|_| l.split_once(':').unwrap().1.trim().to_string()));
                let length: usize = header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
                while body.len() < length {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    body.extend_from_slice(&chunk[..n]);
                }
                let request_line = head.lines().next().unwrap_or_default().to_string();
                let target = request_line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let path = target.split('?').next().unwrap().to_string();
                let query = target.split_once('?').map(|(_, q)| q).unwrap_or("").to_string();
                let body = String::from_utf8_lossy(&body).to_string();
                log.lock().unwrap().push(Seen { path: path.clone(), body: body.clone(), api_key: header("api-key"), api_sign: header("api-sign") });
                let reply = handler(&path, if body.is_empty() { &query } else { &body });
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", reply.len(), reply);
            });
        }
    });
    StandIn { url, seen }
}

fn client(server: &StandIn) -> KrakenClient {
    KrakenClient::with_base_url(Credentials::new("my-key", SECRET).unwrap(), server.url.clone())
}

const ASSETS: &str = r#"{"error":[],"result":{"XXBT":{"altname":"XBT"},"XETH":{"altname":"ETH"},"ZUSD":{"altname":"USD"},"XXDG":{"altname":"XDG"},"DOT":{"altname":"DOT"},"DOT.S":{"altname":"DOT.S"},"USDT":{"altname":"USDT"}}}"#;
const PAIRS: &str = r#"{"error":[],"result":{
 "XXBTZUSD":{"altname":"XBTUSD","base":"XXBT","quote":"ZUSD"},
 "XETHZUSD":{"altname":"ETHUSD","base":"XETH","quote":"ZUSD"},
 "DOTUSD":{"altname":"DOTUSD","base":"DOT","quote":"ZUSD"}}}"#;
const OK_EMPTY: &str = r#"{"error":[],"result":{"count":0,"trades":{},"ledger":{}}}"#;
const DENIED: &str = r#"{"error":["EGeneral:Permission denied"]}"#;

fn read_only_key(path: &str, _body: &str) -> String {
    match path {
        "/0/private/BalanceEx" => r#"{"error":[],"result":{"ZUSD":{"balance":"10"}}}"#.into(),
        "/0/private/TradesHistory" | "/0/private/Ledgers" => OK_EMPTY.into(),
        "/0/private/AddOrder" | "/0/private/WithdrawStatus" => DENIED.into(),
        "/0/public/Assets" => ASSETS.into(),
        "/0/public/AssetPairs" => PAIRS.into(),
        other => panic!("unexpected call to {other}"),
    }
}

#[test]
fn a_read_only_key_is_accepted_and_every_private_call_is_signed_over_the_body_it_sends() {
    let server = stand_in(read_only_key);
    client(&server).ensure_read_only().unwrap();

    let seen = server.seen.lock().unwrap().clone();
    let private: Vec<&Seen> = seen.iter().filter(|s| s.path.starts_with("/0/private/")).collect();
    assert_eq!(private.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(), vec!["/0/private/BalanceEx", "/0/private/TradesHistory", "/0/private/Ledgers", "/0/private/AddOrder", "/0/private/WithdrawStatus"]);
    let signer = Credentials::new("my-key", SECRET).unwrap();
    for call in private {
        assert_eq!(call.api_key.as_deref(), Some("my-key"));
        let nonce = call.body.strip_prefix("nonce=").unwrap().split('&').next().unwrap();
        assert_eq!(call.api_sign.as_deref(), Some(signer.sign(&call.path, nonce, &call.body).as_str()), "{} was signed over something other than its body", call.path);
    }
    // The order that is tried is only ever checked, never placed.
    let order = seen.iter().find(|s| s.path == "/0/private/AddOrder").unwrap();
    assert!(order.body.contains("validate=true"), "{}", order.body);
}

#[test]
fn nonces_grow_from_one_call_to_the_next() {
    let server = stand_in(read_only_key);
    client(&server).ensure_read_only().unwrap();
    let nonces: Vec<u64> = server.seen.lock().unwrap().iter().filter(|s| s.path.starts_with("/0/private/")).map(|s| s.body.strip_prefix("nonce=").unwrap().split('&').next().unwrap().parse().unwrap()).collect();
    assert!(nonces.windows(2).all(|w| w[0] < w[1]), "{nonces:?}");
}

#[test]
fn a_key_that_can_place_orders_is_refused_whether_or_not_the_check_order_was_valid() {
    for order_answer in [r#"{"error":[],"result":{"descr":{"order":"buy 0.0001 XBTUSD @ limit 1"}}}"#, r#"{"error":["EOrder:Insufficient funds"]}"#] {
        let server = stand_in(move |path, body| if path == "/0/private/AddOrder" { order_answer.into() } else { read_only_key(path, body) });
        match client(&server).ensure_read_only() {
            Err(KrakenError::NotReadOnly(what)) => assert!(what.contains("place orders"), "{what}"),
            other => panic!("expected NotReadOnly, got {other:?}"),
        }
    }
}

#[test]
fn a_key_that_can_withdraw_is_refused() {
    let server = stand_in(|path, body| if path == "/0/private/WithdrawStatus" { r#"{"error":[],"result":[]}"#.into() } else { read_only_key(path, body) });
    match client(&server).ensure_read_only() {
        Err(KrakenError::NotReadOnly(what)) => assert!(what.contains("withdraw"), "{what}"),
        other => panic!("expected NotReadOnly, got {other:?}"),
    }
}

#[test]
fn an_answer_that_proves_nothing_about_the_key_is_not_taken_as_read_only() {
    let server = stand_in(|path, body| if path == "/0/private/AddOrder" { r#"{"error":["EService:Unavailable"]}"#.into() } else { read_only_key(path, body) });
    assert!(matches!(client(&server).ensure_read_only(), Err(KrakenError::Unverifiable(_))));
}

#[test]
fn a_key_missing_a_read_permission_is_told_which_one() {
    let server = stand_in(|path, body| if path == "/0/private/Ledgers" { DENIED.into() } else { read_only_key(path, body) });
    match client(&server).ensure_read_only() {
        Err(KrakenError::MissingPermission { permission, said }) => {
            assert!(permission.contains("Query ledger entries") && permission.contains("Data"), "{permission}");
            assert!(said.contains("Permission denied"), "Kraken's own words are kept: {said}");
        }
        other => panic!("expected MissingPermission, got {other:?}"),
    }
    let server = stand_in(|_, _| r#"{"error":["EAPI:Invalid key"]}"#.into());
    assert!(matches!(client(&server).ensure_read_only(), Err(KrakenError::BadKey)));
}

#[test]
fn balances_use_the_usual_names_and_fold_staked_and_held_amounts_into_their_asset() {
    let server = stand_in(|path, body| match path {
        "/0/private/BalanceEx" => r#"{"error":[],"result":{
            "XXBT":{"balance":"0.5","hold_trade":"0.1"},"ZUSD":{"balance":"100.25","hold_trade":"0"},
            "DOT":{"balance":"2","hold_trade":"0"},"DOT.S":{"balance":"8","hold_trade":"0"},"XXDG":{"balance":"1000","hold_trade":"0"}}}"#.into(),
        _ => read_only_key(path, body),
    });
    let balances: BTreeMap<String, (String, String)> = client(&server).fetch_account_balances().unwrap().into_iter().map(|b| (b.asset, (b.free, b.locked))).collect();
    assert_eq!(balances["BTC"], ("0.4".to_string(), "0.1".to_string()));
    assert_eq!(balances["USD"], ("100.25".to_string(), "0".to_string()));
    assert_eq!(balances["DOGE"], ("1000".to_string(), "0".to_string()));
    assert_eq!(balances["DOT"], ("2".to_string(), "8".to_string()), "staked DOT is still DOT, held rather than free");
    assert!(!balances.contains_key("DOT.S") && !balances.contains_key("XXBT"));
}

fn trade_page(ids: &[(&str, &str, f64)], count: usize) -> String {
    let trades: Vec<String> = ids.iter().map(|(id, pair, time)| format!(r#""{id}":{{"ordertxid":"O-{id}","pair":"{pair}","time":{time},"type":"buy","price":"100","vol":"1","fee":"0.5","cost":"100"}}"#)).collect();
    format!(r#"{{"error":[],"result":{{"count":{count},"trades":{{{}}}}}}}"#, trades.join(","))
}

#[test]
fn trades_are_read_page_by_page_under_linvesther_market_names() {
    let server = stand_in(|path, body| match path {
        "/0/private/TradesHistory" if body.contains("ofs=0") => trade_page(&[("T3", "XXBTZUSD", 1789749300.5), ("T2", "DOTUSD", 1789749200.0)], 3),
        "/0/private/TradesHistory" if body.contains("ofs=2") => trade_page(&[("T1", "XETHZUSD", 1789749100.0)], 3),
        _ => read_only_key(path, body),
    });
    let trades = client(&server).trades_since(1_789_749_000_000).unwrap();
    assert_eq!(trades.iter().map(|t| (t.id.as_str(), t.symbol.as_str())).collect::<Vec<_>>(), vec![("T1", "ETH-USD"), ("T2", "DOT-USD"), ("T3", "BTC-USD")], "oldest first, under BASE-QUOTE names");
    assert_eq!(trades[2].time_ms, 1_789_749_300_500);
    let calls: Vec<String> = server.seen.lock().unwrap().iter().filter(|s| s.path == "/0/private/TradesHistory").map(|s| s.body.clone()).collect();
    assert_eq!(calls.len(), 2, "one request per page: {calls:?}");
    assert!(calls[0].contains("start=1789748999"), "asks from one second before, since Kraken's start is exclusive: {}", calls[0]);
}

#[test]
fn the_ledger_is_read_with_assets_in_their_usual_names() {
    let server = stand_in(|path, body| match path {
        "/0/private/Ledgers" => r#"{"error":[],"result":{"count":2,"ledger":{
            "L2":{"refid":"R2","time":1789749200.0,"type":"withdrawal","subtype":"","asset":"XXBT","amount":"-0.5","fee":"0.0005"},
            "L1":{"refid":"R1","time":1789749100.0,"type":"deposit","subtype":"","asset":"ZUSD","amount":"100","fee":"0"}}}}"#.into(),
        _ => read_only_key(path, body),
    });
    let entries = client(&server).ledger_since(1_789_749_000_000).unwrap();
    assert_eq!(entries.iter().map(|e| (e.id.as_str(), e.asset.as_str(), e.kind.as_str())).collect::<Vec<_>>(), vec![("L1", "USD", "deposit"), ("L2", "BTC", "withdrawal")]);
}

#[test]
fn a_rate_limit_answer_is_waited_out_and_retried() {
    let calls = Arc::new(Mutex::new(0));
    let counter = calls.clone();
    let server = stand_in(move |path, body| {
        if path == "/0/private/BalanceEx" {
            let mut n = counter.lock().unwrap();
            *n += 1;
            if *n == 1 {
                return r#"{"error":["EAPI:Rate limit exceeded"]}"#.into();
            }
        }
        read_only_key(path, body)
    });
    assert!(client(&server).fetch_account_balances().is_ok());
    assert_eq!(*calls.lock().unwrap(), 2);
}

#[test]
fn a_trades_market_name_is_the_one_the_engine_looks_its_assets_up_by_and_prices_its_holdings_under() {
    use std::collections::BTreeSet;
    let server = stand_in(|path, body| match path {
        "/0/private/TradesHistory" => trade_page(&[("T1", "XXBTZUSD", 1789749100.0)], 1),
        _ => read_only_key(path, body),
    });
    let client = client(&server);
    let trade = client.trades_since(0).unwrap().remove(0);
    let assets = client.fetch_symbol_assets_for(&BTreeSet::from([trade.symbol.clone()])).unwrap();
    assert_eq!(assets[&trade.symbol], ("BTC".to_string(), "USD".to_string()));
    // The market that prices a holding of that asset is the same market.
    assert_eq!(client.market_symbol(&trade.base), trade.symbol);
    assert_eq!(client.quote_currency(), trade.quote);
}
