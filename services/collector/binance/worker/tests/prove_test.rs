//! Real ZK proving against the live account and its already-collected
//! trades/flows: builds the signed A0 envelope over the real,
//! flow-adjusted TWR index and runs actual RISC Zero proving (not
//! dev-mode) through `zkvm/methods/performance`'s real performance guest.
//!
//! This is the CPU/RAM-heavy step — same class of operation that has
//! frozen this machine before during real proving elsewhere in this
//! project. Run manually, after connecting an account with some real
//! trade history (see `history_test.rs`'s own doc comment for that
//! setup), and only when you can afford a slow, heavy build:
//!
//! ```sh
//! set -a && source .env && set +a
//! export DATABASE_URL="postgresql://linvestherzk:linvestherzk-local-dev-only@localhost:5433/linvestherzk"
//! export BINANCE_WORKER_A0_SIGNING_KEY=$(openssl rand -hex 32)
//! cargo test --manifest-path services/Cargo.toml -p binance-worker --test prove_test -- --ignored --test-threads=1
//! ```

use binance_worker::db;
use binance_worker::prove::prove_performance;
use collector_credentials::Environment;
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
#[ignore = "requires a real Binance API key/secret, a running local Postgres with a real, already-connected account, and real (heavy) RISC Zero proving; see this file's doc comment."]
async fn real_performance_proof_verifies_against_the_real_guest_image_id() {
    let api_key = std::env::var("BINANCE_API_KEY").expect("BINANCE_API_KEY must be set");
    let api_secret = std::env::var("BINANCE_API_SECRET").expect("BINANCE_API_SECRET must be set");
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let signing_key_hex = std::env::var("BINANCE_WORKER_A0_SIGNING_KEY")
        .expect("BINANCE_WORKER_A0_SIGNING_KEY must be set (openssl rand -hex 32)");
    let signing_key: [u8; 32] = hex::decode(&signing_key_hex)
        .expect("BINANCE_WORKER_A0_SIGNING_KEY must be hex")
        .try_into()
        .expect("BINANCE_WORKER_A0_SIGNING_KEY must be exactly 32 bytes");

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect to local test Postgres");
    db::run_migrations(&pool).await.expect("run migrations");

    let connection = db::find_connection_by_account(&pool, "history-test")
        .await
        .expect("query connection")
        .expect("expected a connection named 'history-test' — connect one first, see history_test.rs's doc comment");

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

    eprintln!("starting real RISC Zero proving — this can take a while and uses real CPU/RAM");
    let proof = prove_performance(
        &pool,
        connection.id,
        "history-test",
        std::sync::Arc::new(binance_worker::market::BinanceMarket::new(client)),
        &signing_key,
        now_ms,
    )
    .await
    .expect("real proving of real, collected history must succeed");

    eprintln!("envelope digest: {}", hex::encode(proof.envelope_digest));
    assert_eq!(
        proof.signer_fingerprint,
        collector_attestation::signer::A0Signer::from_bytes(&signing_key)
            .unwrap()
            .fingerprint(),
        "the journal names the collector key this worker signs with"
    );
    eprintln!(
        "period: {} .. {}",
        proof.period_start_ms, proof.period_end_ms
    );
    eprintln!(
        "proven return (fraction, scale 1e6): {}",
        proof.twr_return_scaled
    );
    eprintln!("proven max drawdown (bp): {}", proof.mdd_bp);

    // The receipt this module just produced must independently verify
    // against the real guest's own image ID — the same check any third
    // party (never trusting this test or this worker again) would run.
    proof
        .receipt
        .verify(proof.image_id)
        .expect("the real receipt must verify against its own image ID");
    assert_eq!(proof.image_id, zkvm_methods::GUEST_ID);
}
