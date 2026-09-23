//! Talks to Kraken's real public endpoints (no key needed), so it is never part
//! of the default gate. Run it by hand:
//!
//! ```sh
//! cargo test --manifest-path services/Cargo.toml -p kraken-client --test live_public -- --ignored --nocapture
//! ```

use exchange_core::MarketData;
use kraken_client::{Credentials, KrakenClient};
use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

fn client() -> KrakenClient {
    // Only public endpoints are called, so the credentials are never used to sign.
    KrakenClient::new(Credentials::new("public-only", "AAAA").unwrap())
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

#[test]
#[ignore = "calls Kraken's public API"]
fn markets_are_found_under_linvesther_names() {
    let client = client();
    let wanted: BTreeSet<String> = ["BTC-USD", "ETH-USD", "DOGE-USD", "USDT-USD", "EUR-USD", "NOPE-USD"].iter().map(|s| s.to_string()).collect();
    let found = client.fetch_symbol_assets_for(&wanted).unwrap();
    assert_eq!(found["BTC-USD"], ("BTC".to_string(), "USD".to_string()));
    assert_eq!(found["DOGE-USD"], ("DOGE".to_string(), "USD".to_string()));
    assert_eq!(found["EUR-USD"], ("EUR".to_string(), "USD".to_string()));
    assert!(!found.contains_key("NOPE-USD"));
}

#[test]
#[ignore = "calls Kraken's public API"]
fn candles_of_every_chart_step_cover_the_latest_range() {
    let client = client();
    let now = now_ms();
    for (step, span_ms) in [(300_000u64, 86_400_000u64), (1_800_000, 7 * 86_400_000), (7_200_000, 28 * 86_400_000), (86_400_000, 300 * 86_400_000)] {
        let start = now - span_ms;
        let candles = client.fetch_candles_span("BTC-USD", step, start.saturating_sub(2 * step), now).unwrap();
        assert!(!candles.is_empty(), "no candles for step {step}");
        assert!(candles.windows(2).all(|w| w[0].open_time_ms < w[1].open_time_ms), "candles out of order for step {step}");
        assert!(candles.last().unwrap().open_time_ms + 2 * step >= now, "step {step} stops short of now");
        println!("step {step}: {} candles", candles.len());
    }
}

#[test]
#[ignore = "calls Kraken's public API"]
fn a_price_is_found_for_a_recent_and_for_an_old_instant() {
    let client = client();
    let now = now_ms();
    let recent = client.price_at_instant("BTC", now - 5 * 60_000).unwrap();
    let old = client.price_at_instant("BTC", now - 3 * 86_400_000).unwrap();
    println!("BTC 5 minutes ago: {recent}, 3 days ago: {old}");
    assert!(recent.is_sign_positive() && old.is_sign_positive());
}
