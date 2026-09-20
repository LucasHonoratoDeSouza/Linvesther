//! Real WebSocket connection test against the live Binance API — same
//! `#[ignore]` convention as this crate's other real-credential tests.
//! Run manually:
//!
//! ```sh
//! docker compose -f infra/docker-compose.yml up -d postgres
//! set -a && source .env && set +a
//! export DATABASE_URL="postgresql://linvestherzk:linvestherzk-local-dev-only@localhost:5433/linvestherzk"
//! cargo test --manifest-path services/Cargo.toml -p binance-worker --test stream_test -- --ignored --test-threads=1
//! ```

use binance_worker::db;
use binance_worker::stream::run_trade_stream;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires a real Binance API key/secret and a running local Postgres; see this file's doc comment."]
async fn subscribing_to_the_real_account_stream_succeeds() {
    let api_key = std::env::var("BINANCE_API_KEY").expect("BINANCE_API_KEY must be set");
    let api_secret = std::env::var("BINANCE_API_SECRET").expect("BINANCE_API_SECRET must be set");
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect to local test Postgres");
    db::run_migrations(&pool).await.expect("run migrations");

    let connection_id = db::insert_connection(
        &pool,
        "stream-test-account",
        &binance_worker::crypto::encrypt(
            &binance_worker::crypto::MasterKey::from_hex(&"22".repeat(32)).unwrap(),
            "k",
        )
        .unwrap(),
        &binance_worker::crypto::encrypt(
            &binance_worker::crypto::MasterKey::from_hex(&"22".repeat(32)).unwrap(),
            "s",
        )
        .unwrap(),
        &[],
    )
    .await
    .expect("insert test connection");

    // run_trade_stream only returns on subscribe failure or a dropped
    // connection — it blocks forever on a genuinely successful
    // subscription with no incoming trade. A timeout that elapses
    // (rather than the call returning an error immediately) is itself
    // the proof that the real subscribe request was accepted.
    let result = tokio::time::timeout(
        Duration::from_secs(8),
        run_trade_stream(&pool, connection_id, &api_key, &api_secret),
    )
    .await;

    sqlx::query("DELETE FROM binance_connections WHERE account_id = 'stream-test-account'")
        .execute(&pool)
        .await
        .ok();

    match result {
        Err(_elapsed) => {} // timed out while still listening — subscribe succeeded
        Ok(Err(e)) => panic!(
            "stream ended with an error instead of a successful, still-listening subscription: {e}"
        ),
        Ok(Ok(())) => panic!(
            "stream ended cleanly without error, which should not happen while still listening"
        ),
    }
}
