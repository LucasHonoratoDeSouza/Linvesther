//! The price source against a stand-in for DefiLlama on a local port.

mod common;

use common::{param, stand_in};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wallet_client::{PriceError, PriceSource};

fn source(url: &str) -> PriceSource {
    PriceSource::new(url).with_pacing(Duration::ZERO, Duration::from_millis(5))
}

#[test]
fn a_coin_with_no_price_is_absent_and_many_coins_are_asked_in_batches() {
    let server = stand_in(|query| {
        let _ = query;
        (200, json!({ "coins": {
            "ethereum:0xa": { "decimals": 6, "symbol": "USDC", "price": 0.9999, "timestamp": 1_790_000_000, "confidence": 0.99 },
            "coingecko:ethereum": { "price": 2600.5, "timestamp": 1_790_000_100, "confidence": 0.99 },
            "ethereum:0xspam": { "price": 5.0, "timestamp": 1_790_000_100, "confidence": 0.1 }
        } }).to_string())
    });
    let prices = source(&server.url).current(&["ethereum:0xa".into(), "coingecko:ethereum".into(), "ethereum:0xspam".into(), "ethereum:0xnone".into()]).unwrap();
    assert_eq!(prices.len(), 2, "the unpriced and the doubtful coins are left out");
    assert_eq!(prices["coingecko:ethereum"].usd.to_string(), "2600.5");
    assert_eq!(prices["ethereum:0xa"].at_ms, 1_790_000_000_000);

    let many: Vec<String> = (0..85).map(|i| format!("ethereum:0x{i:040x}")).collect();
    let counted = stand_in(|_| (200, json!({ "coins": {} }).to_string()));
    source(&counted.url).current(&many).unwrap();
    assert_eq!(counted.seen.lock().unwrap().len(), 3, "85 coins in requests of at most 40");
}

#[test]
fn a_past_price_names_its_moment_and_how_far_from_it_the_source_may_look() {
    let server = stand_in(|_| (200, json!({ "coins": {} }).to_string()));
    let _ = source(&server.url).at(&["coingecko:ethereum".into()], 1_789_000_000_000).unwrap();
    // The moment is in the path (in seconds), the tolerance in the query.
    let seen = server.seen.lock().unwrap().clone();
    assert_eq!(param(&seen[0], "searchWidth").as_deref(), Some("2h"));
}

#[test]
fn a_chart_over_a_long_range_is_asked_in_pieces_and_stitched_oldest_first_without_repeats() {
    let server = stand_in(|query| {
        let start: u64 = param(query, "start").unwrap().parse().unwrap();
        let span: u64 = param(query, "span").unwrap().parse().unwrap();
        // Two pieces overlap on the moment where the first ends and the second begins.
        let prices: Vec<_> = (0..span).map(|i| json!({ "timestamp": start + i * 300, "price": 100.0 + ((start + i * 300) % 1000) as f64 })).collect();
        (200, json!({ "coins": { "coingecko:ethereum": { "symbol": "ETH", "confidence": 0.99, "prices": prices } } }).to_string())
    });
    // 700 five-minute points: more than one request holds.
    let end_ms = 1_790_000_000_000 + 699 * 300_000;
    let points = source(&server.url).chart("coingecko:ethereum", 1_790_000_000_000, end_ms, 300_000).unwrap();
    assert_eq!(points.len(), 700);
    assert!(points.windows(2).all(|w| w[0].0 < w[1].0), "oldest first, each moment once");
    assert_eq!(server.seen.lock().unwrap().len(), 2, "500 points, then 200");
}

#[test]
fn a_rate_limited_source_is_waited_out_then_reported() {
    let calls = Arc::new(Mutex::new(0));
    let counter = calls.clone();
    let server = stand_in(move |_| {
        let mut n = counter.lock().unwrap();
        *n += 1;
        if *n == 1 {
            (429, "slow down".into())
        } else {
            (200, json!({ "coins": {} }).to_string())
        }
    });
    assert!(source(&server.url).current(&["coingecko:ethereum".into()]).is_ok());
    assert_eq!(*calls.lock().unwrap(), 2);
    let always = stand_in(|_| (429, "slow down".into()));
    assert!(matches!(source(&always.url).current(&["coingecko:ethereum".into()]), Err(PriceError::RateLimited)));
}

#[test]
#[ignore = "calls DefiLlama's real API"]
fn the_real_source_prices_a_token_now_at_a_past_moment_and_over_a_range() {
    let source = PriceSource::default();
    let usdc = "ethereum:0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    let current = source.current(&[usdc.clone(), "coingecko:ethereum".into(), "ethereum:0x0000000000000000000000000000000000000001".into()]).unwrap();
    assert_eq!(current.len(), 2, "the made-up coin has no price");
    let past = source.at(&["coingecko:ethereum".into()], now - 3 * 86_400_000).unwrap();
    assert!(past["coingecko:ethereum"].usd.is_sign_positive());
    let chart = source.chart("coingecko:ethereum", now - 2 * 86_400_000, now, 3_600_000).unwrap();
    assert!(chart.len() > 24, "{} points", chart.len());
}
