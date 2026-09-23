//! Integration tests against a real local Postgres — never part of the
//! default gate (this repo avoids bringing up `infra/docker-compose.yml`
//! containers in its own gates, per `infra/recovery`'s README). Run
//! manually:
//!
//! ```sh
//! docker compose -f infra/docker-compose.yml up -d postgres
//! export DATABASE_URL="postgresql://linvestherzk:linvestherzk-local-dev-only@localhost:5433/linvestherzk"
//! cargo test --manifest-path services/Cargo.toml -p binance-worker --test db_test -- --ignored --test-threads=1
//! ```

use binance_catalog::CatalogSnapshot;
use binance_flows::{AssetLeg, FlowFamily, NormalizedFlow};
use binance_trades::Trade;
use binance_worker::crypto::{encrypt, MasterKey};
use binance_worker::db;
use sqlx::postgres::PgPoolOptions;
use std::collections::BTreeSet;

async fn pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL").expect(
        "DATABASE_URL must point at a running local Postgres (see this file's doc comment)",
    );
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect to local test Postgres");
    db::run_migrations(&pool).await.expect("run migrations");
    pool
}

fn test_key() -> MasterKey {
    MasterKey::from_hex(&"11".repeat(32)).unwrap()
}

async fn cleanup(pool: &sqlx::PgPool, account_id: &str) {
    sqlx::query("DELETE FROM binance_connections WHERE account_id = $1")
        .bind(account_id)
        .execute(pool)
        .await
        .ok();
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see this file's doc comment for how to start one."]
async fn inserting_a_connection_and_finding_it_by_account_round_trips() {
    let pool = pool().await;
    let account_id = "db-test-round-trip";
    cleanup(&pool, account_id).await;

    let key = test_key();
    let enc_key = encrypt(&key, "sk-test-key", "test").unwrap();
    let enc_secret = encrypt(&key, "sk-test-secret", "test").unwrap();
    let symbols = vec!["BTCUSDT".to_string()];

    let id = db::insert_connection(&pool, account_id, &enc_key, &enc_secret, &symbols)
        .await
        .unwrap();
    let found = db::find_connection_by_account(&pool, account_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.id, id);
    assert_eq!(found.account_id, account_id);
    assert_eq!(found.symbols, symbols);
    assert_eq!(found.status, "active");
    assert!(found.last_synced_at.is_none());

    cleanup(&pool, account_id).await;
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see this file's doc comment."]
async fn connecting_the_same_account_twice_updates_rather_than_duplicates() {
    let pool = pool().await;
    let account_id = "db-test-upsert";
    cleanup(&pool, account_id).await;

    let key = test_key();
    let enc_key = encrypt(&key, "sk-1", "test").unwrap();
    let enc_secret = encrypt(&key, "sk-1-secret", "test").unwrap();
    let first_id = db::insert_connection(
        &pool,
        account_id,
        &enc_key,
        &enc_secret,
        &["BTCUSDT".to_string()],
    )
    .await
    .unwrap();

    let enc_key_2 = encrypt(&key, "sk-2", "test").unwrap();
    let enc_secret_2 = encrypt(&key, "sk-2-secret", "test").unwrap();
    let second_id = db::insert_connection(
        &pool,
        account_id,
        &enc_key_2,
        &enc_secret_2,
        &["ETHUSDT".to_string()],
    )
    .await
    .unwrap();

    assert_eq!(
        first_id, second_id,
        "reconnecting the same account must reuse its connection id"
    );
    let found = db::find_connection_by_account(&pool, account_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        found.symbols,
        vec!["ETHUSDT".to_string()],
        "symbols must reflect the latest connect call"
    );

    cleanup(&pool, account_id).await;
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see this file's doc comment."]
async fn upserting_the_same_trade_twice_does_not_duplicate_it() {
    let pool = pool().await;
    let account_id = "db-test-trade-dedup";
    cleanup(&pool, account_id).await;

    let key = test_key();
    let connection_id = db::insert_connection(
        &pool,
        account_id,
        &encrypt(&key, "k", "test").unwrap(),
        &encrypt(&key, "s", "test").unwrap(),
        &["BTCUSDT".to_string()],
    )
    .await
    .unwrap();

    let trade = Trade {
        symbol: "BTCUSDT".to_string(),
        id: 42,
        order_id: 7,
        price: "50000.00".to_string(),
        qty: "0.001".to_string(),
        commission: "0.00001".to_string(),
        commission_asset: "BTC".to_string(),
        time_ms: 1_700_000_000_000,
        is_buyer: true,
    };

    db::upsert_trades(&pool, connection_id, &[trade.clone()])
        .await
        .unwrap();
    db::upsert_trades(&pool, connection_id, &[trade])
        .await
        .unwrap();

    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM binance_synced_trades WHERE connection_id = $1")
            .bind(connection_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);

    cleanup(&pool, account_id).await;
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see this file's doc comment."]
async fn a_trade_with_the_same_id_under_a_different_symbol_is_a_separate_row() {
    // Mirrors binance-trades' own namespace rule: (symbol, id) is the
    // key, so the same numeric id on two symbols is never a collision.
    let pool = pool().await;
    let account_id = "db-test-namespace";
    cleanup(&pool, account_id).await;

    let key = test_key();
    let connection_id = db::insert_connection(
        &pool,
        account_id,
        &encrypt(&key, "k", "test").unwrap(),
        &encrypt(&key, "s", "test").unwrap(),
        &["BTCUSDT".to_string(), "ETHUSDT".to_string()],
    )
    .await
    .unwrap();

    let base = Trade {
        symbol: "BTCUSDT".to_string(),
        id: 1,
        order_id: 1,
        price: "1".to_string(),
        qty: "1".to_string(),
        commission: "0".to_string(),
        commission_asset: "BTC".to_string(),
        time_ms: 1,
        is_buyer: true,
    };
    let other_symbol = Trade {
        symbol: "ETHUSDT".to_string(),
        ..base.clone()
    };

    db::upsert_trades(&pool, connection_id, &[base, other_symbol])
        .await
        .unwrap();

    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM binance_synced_trades WHERE connection_id = $1")
            .bind(connection_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 2);

    cleanup(&pool, account_id).await;
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see this file's doc comment."]
async fn connection_summary_reflects_persisted_trades_and_flows() {
    let pool = pool().await;
    let account_id = "db-test-summary";
    cleanup(&pool, account_id).await;

    let unconnected = db::connection_summary(&pool, account_id).await.unwrap();
    assert!(!unconnected.connected);

    let key = test_key();
    let connection_id = db::insert_connection(
        &pool,
        account_id,
        &encrypt(&key, "k", "test").unwrap(),
        &encrypt(&key, "s", "test").unwrap(),
        &["BTCUSDT".to_string()],
    )
    .await
    .unwrap();

    let trade = Trade {
        symbol: "BTCUSDT".to_string(),
        id: 1,
        order_id: 1,
        price: "1".to_string(),
        qty: "1".to_string(),
        commission: "0".to_string(),
        commission_asset: "BTC".to_string(),
        time_ms: 1,
        is_buyer: true,
    };
    db::upsert_trades(&pool, connection_id, &[trade])
        .await
        .unwrap();

    let flow = NormalizedFlow {
        source_namespace: FlowFamily::Deposit,
        source_id: "dep-1".to_string(),
        economic_time_ms: 1,
        kind: "Credited".to_string(),
        legs: vec![AssetLeg::credit("USDT", "100")],
        fee: None,
    };
    db::upsert_flows(&pool, connection_id, &[flow])
        .await
        .unwrap();

    let snapshot = CatalogSnapshot {
        captured_at_ms: 1,
        symbols: BTreeSet::from(["BTCUSDT".to_string()]),
    };
    db::upsert_catalog_snapshot(&pool, connection_id, &snapshot)
        .await
        .unwrap();

    db::mark_synced(&pool, connection_id).await.unwrap();

    let summary = db::connection_summary(&pool, account_id).await.unwrap();
    assert!(summary.connected);
    assert_eq!(summary.trade_count, 1);
    assert_eq!(summary.flow_count, 1);
    assert!(summary.last_synced_at.is_some());

    cleanup(&pool, account_id).await;
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see this file's doc comment."]
async fn a_freshly_connected_account_is_immediately_due_for_sync_and_a_just_synced_one_is_not() {
    let pool = pool().await;
    let account_id = "db-test-due-for-sync";
    cleanup(&pool, account_id).await;

    let key = test_key();
    let connection_id = db::insert_connection(
        &pool,
        account_id,
        &encrypt(&key, "k", "test").unwrap(),
        &encrypt(&key, "s", "test").unwrap(),
        &["BTCUSDT".to_string()],
    )
    .await
    .unwrap();

    let due_before = db::connections_due_for_sync(&pool, 900).await.unwrap();
    assert!(
        due_before.iter().any(|c| c.id == connection_id),
        "never-synced connection must be due immediately"
    );

    db::mark_synced(&pool, connection_id).await.unwrap();
    let due_after = db::connections_due_for_sync(&pool, 900).await.unwrap();
    assert!(
        !due_after.iter().any(|c| c.id == connection_id),
        "just-synced connection must not be due again within the interval"
    );

    cleanup(&pool, account_id).await;
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see this file's doc comment."]
async fn deleting_a_connection_removes_its_credential_and_everything_collected() {
    let pool = pool().await;
    let account_id = "db-test-delete";
    cleanup(&pool, account_id).await;

    let key = test_key();
    let enc_key = encrypt(&key, "sk-test-key", "test").unwrap();
    let enc_secret = encrypt(&key, "sk-test-secret", "test").unwrap();
    let id = db::insert_connection(&pool, account_id, &enc_key, &enc_secret, &[]).await.unwrap();
    let trade = Trade {
        symbol: "BTCUSDT".to_string(),
        id: 1,
        order_id: 1,
        price: "1".to_string(),
        qty: "1".to_string(),
        commission: "0".to_string(),
        commission_asset: "BTC".to_string(),
        time_ms: 1,
        is_buyer: true,
    };
    db::upsert_trades(&pool, id, &[trade]).await.unwrap();

    assert!(db::delete_connection(&pool, id).await.unwrap(), "an existing connection is removed");

    assert!(db::find_connection_by_account(&pool, account_id).await.unwrap().is_none());
    let left: (i64,) = sqlx::query_as("SELECT count(*) FROM binance_synced_trades WHERE connection_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left.0, 0, "the history collected for it goes with it");
    assert!(!db::delete_connection(&pool, id).await.unwrap(), "removing it again reports that nothing was there");
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see this file's doc comment."]
async fn rekey_moves_credentials_to_the_new_key_and_leaves_unreadable_ones_alone() {
    use binance_worker::crypto::{credential_context, decrypt};
    use binance_worker::rekey;

    let pool = pool().await;
    let readable = "db-test-rekey-readable";
    let foreign = "db-test-rekey-foreign";
    cleanup(&pool, readable).await;
    cleanup(&pool, foreign).await;

    let old = MasterKey::from_hex(&"11".repeat(32)).unwrap();
    let other = MasterKey::from_hex(&"33".repeat(32)).unwrap();
    let ring = MasterKey::from_hex(&"22".repeat(32))
        .unwrap()
        .with_previous(&"11".repeat(32))
        .unwrap();
    let symbols = vec!["BTCUSDT".to_string()];
    let ctx = |account: &str, field: &str| credential_context("binance", account, field);

    let k = encrypt(&old, "the-key", &ctx(readable, "api_key")).unwrap();
    let s = encrypt(&old, "the-secret", &ctx(readable, "api_secret")).unwrap();
    db::insert_connection(&pool, readable, &k, &s, &symbols).await.unwrap();
    let k = encrypt(&other, "x", &ctx(foreign, "api_key")).unwrap();
    let s = encrypt(&other, "y", &ctx(foreign, "api_secret")).unwrap();
    db::insert_connection(&pool, foreign, &k, &s, &symbols).await.unwrap();

    let before = rekey::check(&pool, &ring).await.unwrap();
    assert!(before.upgraded >= 1 && before.unreadable.contains(&foreign.to_string()));
    let untouched = db::find_connection_by_account(&pool, readable).await.unwrap().unwrap();
    assert!(decrypt(&old, &untouched.encrypted_api_key, &(untouched.nonce_api_key.as_slice().try_into().unwrap()), &ctx(readable, "api_key")).is_ok());

    rekey::rekey(&pool, &ring).await.unwrap();
    let moved = db::find_connection_by_account(&pool, readable).await.unwrap().unwrap();
    let plain = decrypt(&ring, &moved.encrypted_api_secret, &(moved.nonce_api_secret.as_slice().try_into().unwrap()), &ctx(readable, "api_secret")).unwrap();
    assert_eq!(plain, "the-secret");
    let new_only = MasterKey::from_hex(&"22".repeat(32)).unwrap();
    assert!(decrypt(&new_only, &moved.encrypted_api_key, &(moved.nonce_api_key.as_slice().try_into().unwrap()), &ctx(readable, "api_key")).is_ok());

    cleanup(&pool, readable).await;
    cleanup(&pool, foreign).await;
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see this file's doc comment."]
async fn a_kraken_connection_stores_its_credential_encrypted_and_everything_collected_goes_with_it() {
    use binance_flows::FlowFamily;
    use binance_worker::crypto::credential_context;
    use binance_worker::kraken;
    use kraken_client::{LedgerEntry, Trade as KrakenTrade};

    let pool = pool().await;
    let account_id = "db-test-kraken";
    sqlx::query("DELETE FROM kraken_connections WHERE account_id = $1").bind(account_id).execute(&pool).await.unwrap();

    let key = test_key();
    // The secret must be base64, like a real Kraken secret.
    let secret = "kQH5HW/8p1uGOVjbgWA7FunAmGO8lsSUXNsu3eow76sz84Q18fWxnyRzBHCd3pd5nE9qa99HAZtuZuj6F1huXg==";
    let stored_key = encrypt(&key, "kraken-public-key", &credential_context("kraken", account_id, "api_key")).unwrap();
    let stored_secret = encrypt(&key, secret, &credential_context("kraken", account_id, "api_secret")).unwrap();
    let id = kraken::insert_connection(&pool, account_id, &stored_key, &stored_secret).await.unwrap();

    let connection = kraken::find_by_account(&pool, account_id).await.unwrap().unwrap();
    assert!(kraken::credentials(&key, &connection).is_ok(), "the stored credential decrypts under its own account");
    let moved = kraken::KrakenConnection { account_id: "someone-else".to_string(), ..connection };
    assert!(kraken::credentials(&key, &moved).is_err(), "it does not decrypt under another account");

    let trade = KrakenTrade {
        id: "TX1".into(), order_id: "O1".into(), symbol: "BTC-USD".into(), base: "BTC".into(), quote: "USD".into(),
        is_buy: true, price: "50000".into(), volume: "0.01".into(), fee: "0.8".into(), time_ms: 5_000,
    };
    kraken::insert_trades(&pool, id, std::slice::from_ref(&trade)).await.unwrap();
    kraken::insert_trades(&pool, id, &[trade]).await.unwrap();
    assert_eq!(kraken::stored_trade_count(&pool, id).await.unwrap(), 1, "the same trade twice is one trade");
    let trades = kraken::trades(&pool, id).await.unwrap();
    assert_eq!((trades[0].symbol.as_str(), trades[0].qty.as_str(), trades[0].commission_asset.as_str(), trades[0].is_buyer), ("BTC-USD", "0.01", "USD", true));

    let line = |id: &str, kind: &str, asset: &str, amount: &str, time_ms: u64| LedgerEntry {
        id: id.into(), refid: format!("R-{id}"), time_ms, kind: kind.into(), subtype: String::new(), asset: asset.into(), amount: amount.into(), fee: "0".into(),
    };
    let ledger = [line("L1", "deposit", "USD", "1000", 1_000), line("L2", "trade", "BTC", "0.01", 5_000), line("L3", "withdrawal", "USD", "-200", 9_000)];
    kraken::insert_ledger(&pool, id, &ledger).await.unwrap();
    kraken::insert_ledger(&pool, id, &ledger).await.unwrap();
    let flows = kraken::flows(&pool, id).await.unwrap();
    assert_eq!(flows.iter().map(|f| f.source_namespace).collect::<Vec<_>>(), vec![FlowFamily::Deposit, FlowFamily::Withdrawal], "the trade line is left to the trade history");

    assert!(kraken::delete_connection(&pool, id).await.unwrap());
    assert!(kraken::find_by_account(&pool, account_id).await.unwrap().is_none());
    for table in ["kraken_trades", "kraken_ledger"] {
        let left: (i64,) = sqlx::query_as(&format!("SELECT count(*) FROM {table} WHERE connection_id = $1")).bind(id).fetch_one(&pool).await.unwrap();
        assert_eq!(left.0, 0, "{table} goes with the connection");
    }
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see this file's doc comment."]
async fn rekey_also_covers_kraken_credentials() {
    use binance_worker::crypto::{credential_context, decrypt};
    use binance_worker::{kraken, rekey};

    let pool = pool().await;
    let account_id = "db-test-kraken-rekey";
    sqlx::query("DELETE FROM kraken_connections WHERE account_id = $1").bind(account_id).execute(&pool).await.unwrap();

    let old = MasterKey::from_hex(&"11".repeat(32)).unwrap();
    let ring = MasterKey::from_hex(&"22".repeat(32)).unwrap().with_previous(&"11".repeat(32)).unwrap();
    let context = |field: &str| credential_context("kraken", account_id, field);
    let stored_key = encrypt(&old, "the-key", &context("api_key")).unwrap();
    let stored_secret = encrypt(&old, "the-secret", &context("api_secret")).unwrap();
    kraken::insert_connection(&pool, account_id, &stored_key, &stored_secret).await.unwrap();

    rekey::rekey(&pool, &ring).await.unwrap();

    let moved = kraken::find_by_account(&pool, account_id).await.unwrap().unwrap();
    let new_only = MasterKey::from_hex(&"22".repeat(32)).unwrap();
    let nonce: [u8; 12] = moved.nonce_api_secret.clone().try_into().unwrap();
    assert_eq!(decrypt(&new_only, &moved.encrypted_api_secret, &nonce, &context("api_secret")).unwrap(), "the-secret");
    sqlx::query("DELETE FROM kraken_connections WHERE account_id = $1").bind(account_id).execute(&pool).await.unwrap();
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see this file's doc comment."]
async fn kraken_is_not_called_again_when_the_last_sync_or_check_was_recent_and_never_by_two_syncs_at_once() {
    use binance_worker::crypto::credential_context;
    use binance_worker::kraken;
    use kraken_client::{Credentials, KrakenClient};
    use std::sync::Arc;

    let pool = pool().await;
    let account_id = "db-test-kraken-gating";
    sqlx::query("DELETE FROM kraken_connections WHERE account_id = $1").bind(account_id).execute(&pool).await.unwrap();
    let key = test_key();
    let secret = "kQH5HW/8p1uGOVjbgWA7FunAmGO8lsSUXNsu3eow76sz84Q18fWxnyRzBHCd3pd5nE9qa99HAZtuZuj6F1huXg==";
    let stored_key = encrypt(&key, "k", &credential_context("kraken", account_id, "api_key")).unwrap();
    let stored_secret = encrypt(&key, secret, &credential_context("kraken", account_id, "api_secret")).unwrap();
    let id = kraken::insert_connection(&pool, account_id, &stored_key, &stored_secret).await.unwrap();
    let connection = kraken::find_by_account(&pool, account_id).await.unwrap().unwrap();
    // Nothing listens here, so any call to Kraken fails: success means no call was made.
    // A blocking HTTP client has to be built where blocking is allowed, as the worker does.
    let secret_owned = secret.to_string();
    let unreachable = || {
        let secret = secret_owned.clone();
        async move {
            tokio::task::spawn_blocking(move || Arc::new(KrakenClient::with_base_url(Credentials::new("k", &secret).unwrap(), "http://127.0.0.1:9")))
                .await
                .unwrap()
        }
    };

    // While another request is already checking the key, a second does not try it too.
    {
        let mut holder = pool.acquire().await.unwrap();
        let verify_key = i64::from_le_bytes(id.as_bytes()[..8].try_into().unwrap()) ^ 0x5eed_c0de;
        let (held,): (bool,) = sqlx::query_as("SELECT pg_try_advisory_lock($1)").bind(verify_key).fetch_one(&mut *holder).await.unwrap();
        assert!(held);
        assert!(kraken::verify_if_due(&pool, id, unreachable().await).await.is_ok(), "the check under way is relied on");
        sqlx::query("SELECT pg_advisory_unlock($1)").bind(verify_key).execute(&mut *holder).await.unwrap();
    }

    // A key never checked is checked; one checked a moment ago is not checked again.
    assert!(kraken::verify_if_due(&pool, id, unreachable().await).await.is_err(), "it tried to reach Kraken");
    kraken::mark_verified(&pool, id).await.unwrap();
    assert!(kraken::verify_if_due(&pool, id, unreachable().await).await.is_ok(), "it trusted the recent check");

    // A connection synced a moment ago is left alone unless a sync is asked for outright.
    sqlx::query("UPDATE kraken_connections SET last_synced_at = now() WHERE id = $1").bind(id).execute(&pool).await.unwrap();
    assert!(kraken::sync(&pool, &connection, unreachable().await, false).await.is_ok(), "a recent sync is reused");
    assert!(kraken::sync(&pool, &connection, unreachable().await, true).await.is_err(), "a forced sync goes to Kraken");

    // While another sync of the same connection holds its lock, a second does nothing.
    let mut holder = pool.acquire().await.unwrap();
    let lock_key = i64::from_le_bytes(id.as_bytes()[..8].try_into().unwrap());
    let (held,): (bool,) = sqlx::query_as("SELECT pg_try_advisory_lock($1)").bind(lock_key).fetch_one(&mut *holder).await.unwrap();
    assert!(held);
    assert!(kraken::sync(&pool, &connection, unreachable().await, true).await.is_ok(), "a sync already under way is enough");
    sqlx::query("SELECT pg_advisory_unlock($1)").bind(lock_key).execute(&mut *holder).await.unwrap();
    drop(holder);
    assert!(kraken::sync(&pool, &connection, unreachable().await, true).await.is_err(), "and once it is done the lock is free again");

    sqlx::query("DELETE FROM kraken_connections WHERE account_id = $1").bind(account_id).execute(&pool).await.unwrap();
}
