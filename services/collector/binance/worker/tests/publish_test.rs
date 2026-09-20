//! Real NAV computation against the live Binance account. Same
//! `#[ignore]` convention as this crate's other real-credential tests.
//! Run manually:
//!
//! ```sh
//! set -a && source .env && set +a
//! cargo test --manifest-path services/Cargo.toml -p binance-worker --test publish_test -- --ignored --test-threads=1
//! ```

use binance_worker::publish::compute_current_nav;
use collector_credentials::Environment;

#[test]
#[ignore = "requires a real Binance API key/secret; see this file's doc comment."]
fn real_nav_computes_from_real_balance_and_real_prices_or_names_the_unsupported_asset() {
    let api_key = std::env::var("BINANCE_API_KEY").expect("BINANCE_API_KEY must be set");
    let api_secret = std::env::var("BINANCE_API_SECRET").expect("BINANCE_API_SECRET must be set");
    let mut client =
        binance_live_client::LiveClient::new(api_key, api_secret, Environment::Production);
    client
        .ensure_read_only()
        .expect("read-only confirmation must succeed first");

    let market = binance_worker::market::BinanceMarket::new(client);
    match compute_current_nav(&market) {
        Ok(result) => {
            let valuation = &result.valuation;
            assert!(
                valuation.nav > rust_decimal::Decimal::ZERO,
                "the real test account's NAV should be positive"
            );
            for asset in &valuation.assets {
                assert!(
                    asset.quantity > rust_decimal::Decimal::ZERO,
                    "{} was included with a non-positive quantity",
                    asset.asset
                );
                assert!(
                    asset.price > rust_decimal::Decimal::ZERO,
                    "{} was included with a non-positive price",
                    asset.asset
                );
            }
            eprintln!(
                "real NAV: {} {}",
                valuation.nav,
                binance_worker::market::QUOTE_CURRENCY
            );
            for asset in &valuation.assets {
                eprintln!(
                    "  {} qty={} price={} value={}",
                    asset.asset, asset.quantity, asset.price, asset.value
                );
            }
            if !result.excluded_out_of_scope_assets.is_empty() {
                eprintln!(
                    "excluded (out of spot v0.1 scope): {:?}",
                    result.excluded_out_of_scope_assets
                );
            }
        }
        Err(e) => {
            // A held asset with no USDT market (e.g. a fiat balance like
            // BRL) legitimately refuses the whole NAV per
            // valuation::value_at_cut's own documented "refuse entirely,
            // never partially" behavior — a real, expected finding for
            // this test account, not a bug. Confirm it's specifically
            // that failure mode, not something else.
            let message = e.to_string();
            assert!(
                message.contains("no price support"),
                "expected an UnsupportedAsset refusal, got: {message}"
            );
            eprintln!("NAV refused as expected (asset with no price support): {message}");
        }
    }
}
