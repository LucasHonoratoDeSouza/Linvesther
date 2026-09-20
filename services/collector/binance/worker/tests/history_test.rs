//! Real historical NAV/TWR reconstruction against the live account and
//! its already-collected trades/flows. Same `#[ignore]` convention as
//! this crate's other real-credential tests. Run manually (after
//! connecting an account with some real trade history):
//!
//! ```sh
//! set -a && source .env && set +a
//! export DATABASE_URL="postgresql://linvestherzk:linvestherzk-local-dev-only@localhost:5433/linvestherzk"
//! cargo test --manifest-path services/Cargo.toml -p binance-worker --test history_test -- --ignored --test-threads=1
//! ```

use binance_worker::db;
use binance_worker::history::load_and_compute_history;
use collector_credentials::Environment;
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
#[ignore = "requires a real Binance API key/secret and a running local Postgres with a real, already-connected account; see this file's doc comment."]
async fn real_history_reconstructs_from_real_collected_data_or_reports_no_data() {
    let api_key = std::env::var("BINANCE_API_KEY").expect("BINANCE_API_KEY must be set");
    let api_secret = std::env::var("BINANCE_API_SECRET").expect("BINANCE_API_SECRET must be set");
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect to local test Postgres");
    db::run_migrations(&pool).await.expect("run migrations");

    let connection = db::find_connection_by_account(&pool, "history-test")
        .await
        .expect("query connection")
        .expect("expected a connection named 'history-test' — connect one first, see this file's doc comment");

    // ensure_read_only makes a blocking HTTP call; run it on a blocking
    // task so LiveClient's internal reqwest runtime doesn't nest inside
    // this async test (same reason load_and_compute_history does the
    // same internally for its own blocking calls).
    let client = tokio::task::spawn_blocking(move || {
        let mut client =
            binance_live_client::LiveClient::new(api_key, api_secret, Environment::Production);
        client
            .ensure_read_only()
            .expect("read-only confirmation must succeed first");
        client
    })
    .await
    .expect("blocking task must not panic");
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let history = load_and_compute_history(
        &pool,
        connection.id,
        std::sync::Arc::new(binance_worker::market::BinanceMarket::new(client)),
        now_ms,
    )
        .await
        .expect("real history reconstruction must succeed");

    eprintln!(
        "earliest reliable checkpoint: {:?}",
        history.earliest_reliable_checkpoint_ms
    );
    eprintln!("daily NAV points: {}", history.daily_nav.len());
    for point in &history.daily_nav {
        eprintln!("  {} -> {}", point.day_start_ms, point.nav);
    }
    eprintln!(
        "daily returns (TWR subperiods): {:?}",
        history.daily_returns
    );

    // Real Sharpe/Sortino/MDD/CAGR against real data — proves the
    // handoff from `history.rs`'s real return series into
    // `crates/domain/metrics`' already-tested computations, not just
    // that the series itself was built.
    match metrics::sharpe(&history.daily_returns, rust_decimal::Decimal::ZERO) {
        Ok(sharpe) => eprintln!("real Sharpe (rf=0): {sharpe}"),
        Err(e) => eprintln!("Sharpe unavailable: {e}"),
    }
    match metrics::sortino(&history.daily_returns, rust_decimal::Decimal::ZERO) {
        Ok(sortino) => eprintln!("real Sortino (MAR=0): {sortino}"),
        Err(e) => eprintln!("Sortino unavailable: {e}"),
    }
    if let Some(episode) = metrics::max_drawdown(&history.twr_index) {
        eprintln!("real MDD (on the TWR index, deposits excluded): {episode:?}");
    } else {
        eprintln!("MDD unavailable: empty TWR index");
    }
    if let (Some(first), Some(last)) = (history.twr_index.first(), history.twr_index.last()) {
        let days_elapsed = ((last.time_ms - first.time_ms) / (24 * 60 * 60 * 1000)) as u32;
        match metrics::compute_cagr(first.index, last.index, days_elapsed) {
            Ok(cagr) => eprintln!("real CAGR ({days_elapsed} days elapsed): {cagr}"),
            Err(e) => eprintln!("CAGR unavailable ({days_elapsed} days elapsed): {e}"),
        }
    }

    assert!(
        !history.daily_nav.is_empty(),
        "expected at least one NAV checkpoint given this account has real trade history"
    );
    for point in &history.daily_nav {
        assert!(
            point.nav >= rust_decimal::Decimal::ZERO,
            "NAV should never be negative"
        );
    }
}
