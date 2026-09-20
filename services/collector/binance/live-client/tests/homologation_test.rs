//! Gate B0 homologation: real, credentialed runs against a live Binance
//! account. Per the Binance adapter specification, this is the run every
//! other Binance crate's README names as the missing piece.
//!
//! Reads `BINANCE_API_KEY`/`BINANCE_API_SECRET` from the environment —
//! **never** hardcoded, never logged. Every test is `#[ignore]`d: it
//! requires a real credential and makes real network calls, so it must
//! never run as part of `make check-integration` or any other default
//! gate. Run explicitly with:
//!
//! ```sh
//! set -a && source .env && set +a
//! cargo test --manifest-path services/Cargo.toml -p binance-live-client -- --ignored --test-threads=1
//! ```

use binance_live_client::LiveClient;
use collector_credentials::Environment;

fn client_from_env() -> LiveClient {
    let key = std::env::var("BINANCE_API_KEY")
        .expect("BINANCE_API_KEY must be set (see .env, never hardcoded)");
    let secret = std::env::var("BINANCE_API_SECRET")
        .expect("BINANCE_API_SECRET must be set (see .env, never hardcoded)");
    LiveClient::new(key, secret, Environment::Production)
}

#[test]
#[ignore = "requires a real Binance API key/secret in the environment and makes live network calls — never part of the default gate."]
fn a_real_key_is_confirmed_read_only_before_anything_else_is_allowed() {
    let mut client = client_from_env();
    let restrictions = client
        .ensure_read_only()
        .expect("the configured key must be genuinely read-only");
    assert!(restrictions.enable_reading);
    assert!(!restrictions.enable_withdrawals);
    assert!(!restrictions.enable_spot_and_margin_trading);
    assert!(!restrictions.enable_internal_transfer);
    assert!(!restrictions.enable_universal_transfer);
}

#[test]
#[ignore = "requires a real Binance API key/secret; see the module doc comment."]
fn calling_anything_before_ensure_read_only_is_refused() {
    // Every account-scoped call is refused before ensure_read_only runs
    // — even exchangeInfo (public market data), since the gate is a
    // simple, uniform "confirmed-safe, or nothing runs" rather than a
    // per-endpoint exemption list that could be gotten wrong later.
    let client = client_from_env();
    let exchange_info_result = client.fetch_exchange_info();
    assert!(matches!(
        exchange_info_result,
        Err(binance_live_client::LiveClientError::ReadOnlyNotConfirmed)
    ));
    let trades_result = client.fetch_my_trades("BTCUSDT");
    assert!(matches!(
        trades_result,
        Err(binance_live_client::LiveClientError::ReadOnlyNotConfirmed)
    ));
}

#[test]
#[ignore = "requires a real Binance API key/secret; see the module doc comment."]
fn real_exchange_info_parses_into_a_catalog_snapshot_with_a_nonempty_symbol_universe() {
    let mut client = client_from_env();
    client
        .ensure_read_only()
        .expect("read-only confirmation must succeed first");
    let snapshot = client
        .fetch_exchange_info()
        .expect("a real exchangeInfo call must succeed and parse");
    assert!(
        !snapshot.symbols.is_empty(),
        "the real exchange must list at least one symbol"
    );
    assert!(
        snapshot.symbols.contains("BTCUSDT"),
        "BTCUSDT should exist in the real symbol universe"
    );
}

#[test]
#[ignore = "requires a real Binance API key/secret; see the module doc comment."]
fn real_my_trades_for_a_common_symbol_parses_without_error() {
    let mut client = client_from_env();
    client
        .ensure_read_only()
        .expect("read-only confirmation must succeed first");
    let trades = client
        .fetch_my_trades("BTCUSDT")
        .expect("a real myTrades call must succeed and parse, even if empty");
    // No assertion on trades.len(): the account may genuinely have never
    // traded BTCUSDT. Parsing without error, for real response shapes
    // (including the empty-array case), is the thing being confirmed.
    let _ = trades;
}

#[test]
#[ignore = "requires a real Binance API key/secret; see the module doc comment."]
fn real_deposit_and_withdrawal_history_normalize_without_error() {
    let mut client = client_from_env();
    client
        .ensure_read_only()
        .expect("read-only confirmation must succeed first");
    let deposits = client
        .fetch_deposits()
        .expect("real deposit history must normalize without an unrecognized-status error");
    let withdrawals = client
        .fetch_withdrawals()
        .expect("real withdrawal history must normalize without an unrecognized-status error");
    let _ = (deposits, withdrawals);
}

#[test]
#[ignore = "requires a real Binance API key/secret; see the module doc comment."]
fn real_account_balances_parse_and_include_at_least_one_nonzero_asset() {
    let mut client = client_from_env();
    client
        .ensure_read_only()
        .expect("read-only confirmation must succeed first");
    let balances = client
        .fetch_account_balances()
        .expect("a real account balances call must succeed and parse");
    assert!(
        !balances.is_empty(),
        "the real account must report at least one balance entry"
    );
    let has_nonzero = balances
        .iter()
        .any(|b| b.free != "0.00000000" || b.locked != "0.00000000");
    assert!(
        has_nonzero,
        "expected at least one nonzero balance on the real test account"
    );
}

#[test]
#[ignore = "requires a real Binance API key/secret; see the module doc comment."]
fn real_klines_parse_into_candles_with_a_closed_recent_one() {
    let mut client = client_from_env();
    client
        .ensure_read_only()
        .expect("read-only confirmation must succeed first");
    let candles = client
        .fetch_klines("BTCUSDT", "1m", 5)
        .expect("a real klines call must succeed and parse");
    assert_eq!(candles.len(), 5);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let has_recently_closed = candles
        .iter()
        .any(|c| c.close_time_ms <= now_ms && now_ms - c.close_time_ms < 120_000);
    assert!(
        has_recently_closed,
        "expected at least one candle closed within the last 120s"
    );
}
