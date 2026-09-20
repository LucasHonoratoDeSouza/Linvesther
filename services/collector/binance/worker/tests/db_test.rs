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
    let enc_key = encrypt(&key, "sk-test-key").unwrap();
    let enc_secret = encrypt(&key, "sk-test-secret").unwrap();
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
    let enc_key = encrypt(&key, "sk-1").unwrap();
    let enc_secret = encrypt(&key, "sk-1-secret").unwrap();
    let first_id = db::insert_connection(
        &pool,
        account_id,
        &enc_key,
        &enc_secret,
        &["BTCUSDT".to_string()],
    )
    .await
    .unwrap();

    let enc_key_2 = encrypt(&key, "sk-2").unwrap();
    let enc_secret_2 = encrypt(&key, "sk-2-secret").unwrap();
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
        &encrypt(&key, "k").unwrap(),
        &encrypt(&key, "s").unwrap(),
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
        &encrypt(&key, "k").unwrap(),
        &encrypt(&key, "s").unwrap(),
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
        &encrypt(&key, "k").unwrap(),
        &encrypt(&key, "s").unwrap(),
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
        &encrypt(&key, "k").unwrap(),
        &encrypt(&key, "s").unwrap(),
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
    let enc_key = encrypt(&key, "sk-test-key").unwrap();
    let enc_secret = encrypt(&key, "sk-test-secret").unwrap();
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
