//! Wallet connections against a real Postgres and a synthetic chain: what is stored, what a
//! sync keeps, and that the address stays private. Never part of the default gate; run it the
//! way `db_test.rs` describes.

use binance_flows::FlowFamily;
use binance_worker::crypto::{credential_context, decrypt, MasterKey};
use binance_worker::wallet::{self, ReaderSource, WalletConnection};
use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;
use std::str::FromStr;
use std::sync::Arc;
use wallet_client::synthetic::{SyntheticChain, SyntheticPrices};
use wallet_client::wire::{InternalTx, NormalTx, TokenTransfer};
use wallet_client::{Chain, ChainReader};

const OWNER: &str = "0xf493ad63385d04e5f638b3ca4672138a983cc4c8";
const OTHER_OWNER: &str = "0x1111111111111111111111111111111111111111";
const WALLET: &str = "0xd8da6bf26964af9d7eed9e03e53415d37aa96045";
const OTHER: &str = "0xbbbb";
const ETH: u128 = 1_000_000_000_000_000_000;

fn d(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap()
}

async fn pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must point at a running local Postgres");
    let pool = PgPoolOptions::new().max_connections(5).connect(&url).await.expect("connect to local test Postgres");
    binance_worker::db::run_migrations(&pool).await.expect("run migrations");
    pool
}

fn key() -> MasterKey {
    MasterKey::from_hex(&"11".repeat(32)).unwrap()
}

async fn cleanup(pool: &sqlx::PgPool, account_id: &str) {
    sqlx::query("DELETE FROM wallet_connections WHERE account_id = $1").bind(account_id).execute(pool).await.unwrap();
}

/// Every wallet the tests' owners have, so a test that failed midway cannot make the next one
/// find its wallet already connected.
async fn start_clean(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM wallet_connections WHERE owner_address = ANY($1)").bind(vec![OWNER.to_string(), OTHER_OWNER.to_string()]).execute(pool).await.unwrap();
}

/// One synthetic chain per network, however many are asked for.
struct Chains(Vec<(u64, SyntheticChain)>);

impl ReaderSource for Chains {
    fn reader(&self, chain: &'static Chain) -> Result<Box<dyn ChainReader + Send>, String> {
        self.0.iter().find(|(id, _)| *id == chain.id).map(|(_, c)| Box::new(c.clone()) as Box<dyn ChainReader + Send>).ok_or_else(|| format!("no reader for {}", chain.name))
    }
}

fn receive(hash: &str, block: u64, wei: u128) -> NormalTx {
    NormalTx { hash: hash.into(), block, time_ms: block * 12_000, from: OTHER.into(), to: WALLET.into(), value: wei.to_string(), gas_used: "21000".into(), gas_price: "1000000000".into(), failed: false }
}

fn token_out(hash: &str, block: u64, contract: &str, raw: u128) -> TokenTransfer {
    TokenTransfer { hash: hash.into(), block, time_ms: block * 12_000, contract: contract.into(), symbol: "USDC".into(), decimals: 6, from: WALLET.into(), to: OTHER.into(), value: raw.to_string(), log_index: None }
}

fn ethereum() -> &'static Chain {
    Chain::by_id(1).unwrap()
}

async fn connect(pool: &sqlx::PgPool, account_id: &str, chain: &SyntheticChain, prices: &Arc<SyntheticPrices>) -> WalletConnection {
    wallet::insert_connection(pool, &key(), account_id, WALLET, Some("test")).await.unwrap();
    let connection = wallet::find_by_account(pool, account_id).await.unwrap().unwrap();
    let readers: Arc<dyn ReaderSource> = Arc::new(Chains(vec![(1, chain.clone())]));
    wallet::sync(pool, &connection, WALLET, &[ethereum()], readers, prices.clone(), 1_000, true).await.unwrap();
    connection
}

async fn later(pool: &sqlx::PgPool, connection: &WalletConnection, chain: &SyntheticChain, prices: &Arc<SyntheticPrices>, now_ms: u64) -> Result<(), String> {
    let readers: Arc<dyn ReaderSource> = Arc::new(Chains(vec![(1, chain.clone())]));
    let followed = wallet::followed_chains(pool, connection.id).await.unwrap();
    wallet::sync(pool, connection, WALLET, &followed, readers, prices.clone(), now_ms, true).await
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see db_test.rs for how to start one."]
async fn the_address_is_stored_encrypted_bound_to_its_account_and_one_owner_connects_it_once() {
    let pool = pool().await;
    let account = format!("{OWNER}_wallet-a");
    let second = format!("{OWNER}_wallet-b");
    let elsewhere = format!("{OTHER_OWNER}_wallet");
    start_clean(&pool).await;
    wallet::insert_connection(&pool, &key(), &account, WALLET, Some("main")).await.unwrap();
    let connection = wallet::find_by_account(&pool, &account).await.unwrap().unwrap();

    let raw: Vec<u8> = connection.encrypted_address.clone();
    assert!(!String::from_utf8_lossy(&raw).contains(&WALLET[2..]), "the address is not in the stored bytes");
    let (visible,): (String,) = sqlx::query_as("SELECT (t.*)::text FROM wallet_connections t WHERE id = $1").bind(connection.id).fetch_one(&pool).await.unwrap();
    assert!(!visible.to_lowercase().contains(&WALLET[2..]), "nor anywhere in the row: {visible}");
    assert_eq!(wallet::stored_address(&key(), &connection).unwrap(), WALLET);
    let moved = WalletConnection { account_id: elsewhere.clone(), ..connection };
    assert!(wallet::stored_address(&key(), &moved).is_err(), "it does not decrypt under another account");
    let nonce: [u8; 12] = moved.nonce_address.clone().try_into().unwrap();
    assert!(decrypt(&key(), &moved.encrypted_address, &nonce, &credential_context("wallet", &account, "address")).is_ok());

    // The same owner cannot connect the same wallet under another account; someone else can.
    let error = wallet::insert_connection(&pool, &key(), &second, WALLET, None).await.unwrap_err();
    assert!(error.contains("already connected") && !error.contains(&WALLET[2..]), "{error}");
    wallet::insert_connection(&pool, &key(), &elsewhere, WALLET, None).await.unwrap();
    for a in [&account, &second, &elsewhere] {
        cleanup(&pool, a).await;
    }
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see db_test.rs for how to start one."]
async fn a_sync_keeps_what_became_final_and_the_balances_are_explained() {
    let pool = pool().await;
    let account = format!("{OWNER}_wallet-sync");
    start_clean(&pool).await;
    let chain = SyntheticChain::new(1000);
    chain.open_with_native(5 * ETH);
    chain.open_with_token("0xusdc", "USDC", 6, 100_000_000);
    let prices = Arc::new(SyntheticPrices::new());
    prices.set("coingecko:ethereum", d("2600"));
    prices.set("ethereum:0xusdc", d("1"));
    let connection = connect(&pool, &account, &chain, &prices).await;

    let states = wallet::chain_states(&pool, connection.id).await.unwrap();
    assert_eq!((states.len(), states[0].synced_block), (1, 968), "history starts at the newest final block");
    assert_eq!(wallet::effect_count(&pool, connection.id).await.unwrap(), 0);

    chain.push_normal(receive("0xin", 1010, ETH));
    chain.push_token(token_out("0xsell", 1020, "0xusdc", 100_000_000));
    chain.set_head(1100);
    later(&pool, &connection, &chain, &prices, 2_000).await.unwrap();

    let flows = wallet::flows(&pool, connection.id).await.unwrap();
    let families: Vec<FlowFamily> = flows.iter().map(|f| f.source_namespace).collect();
    assert_eq!(families, vec![FlowFamily::Deposit, FlowFamily::Withdrawal], "ether in, tokens out: {flows:?}");
    let anchors = wallet::anchors(&pool, connection.id).await.unwrap();
    let native = anchors.iter().find(|a| a.asset == "ethereum:native").unwrap();
    assert_eq!(d(&native.quantity), d("6"), "5 + 1 received, and no unexplained difference");
    assert_eq!(d(&anchors.iter().find(|a| a.asset == "ethereum:0xusdc").unwrap().quantity), Decimal::ZERO);
    let rows = wallet::effects(&pool, connection.id).await.unwrap();
    assert!(rows.iter().all(|r| r.kind != "unexplained"), "the transfers explained everything");

    // Reading again with nothing new final changes nothing.
    let before = wallet::effect_count(&pool, connection.id).await.unwrap();
    later(&pool, &connection, &chain, &prices, 3_000).await.unwrap();
    assert_eq!(wallet::effect_count(&pool, connection.id).await.unwrap(), before);
    cleanup(&pool, &account).await;
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see db_test.rs for how to start one."]
async fn transfers_the_explorer_missed_are_recorded_as_money_in_never_as_a_gain() {
    let pool = pool().await;
    let account = format!("{OWNER}_wallet-missing");
    start_clean(&pool).await;
    let chain = SyntheticChain::new(1000);
    chain.open_with_native(ETH);
    chain.open_with_token("0xusdc", "USDC", 6, 100_000_000);
    let prices = Arc::new(SyntheticPrices::new());
    prices.set("coingecko:ethereum", d("2600"));
    prices.set("ethereum:0xusdc", d("1"));
    let connection = connect(&pool, &account, &chain, &prices).await;

    // A swap whose ether leg arrives as an internal transfer the explorer has not indexed.
    chain.push_token(token_out("0xswap", 1010, "0xusdc", 100_000_000));
    chain.push_internal(InternalTx { hash: "0xswap".into(), block: 1010, time_ms: 1010 * 12_000, from: OTHER.into(), to: WALLET.into(), value: (ETH / 25).to_string(), index: "1".into(), failed: false });
    chain.hide_internal_transfers(true);
    chain.set_head(1100);
    later(&pool, &connection, &chain, &prices, 20_000_000).await.unwrap();

    let rows = wallet::effects(&pool, connection.id).await.unwrap();
    let missing: Vec<_> = rows.iter().filter(|r| r.kind == "unexplained").collect();
    assert_eq!(missing.len(), 1);
    assert_eq!((missing[0].asset.as_str(), d(&missing[0].delta)), ("ethereum:native", d("0.04")));
    let flows = wallet::flows(&pool, connection.id).await.unwrap();
    assert_eq!(flows.iter().map(|f| f.source_namespace).collect::<Vec<_>>(), vec![FlowFamily::Withdrawal, FlowFamily::Deposit], "the tokens that left and the ether that came back, both capital");
    let states = wallet::chain_states(&pool, connection.id).await.unwrap();
    assert!(states[0].transfers_incomplete, "the explorer's warning is kept");
    cleanup(&pool, &account).await;
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see db_test.rs for how to start one."]
async fn the_figures_are_refused_for_a_token_that_was_moved_and_cannot_be_priced_but_not_for_noise() {
    let pool = pool().await;
    let account = format!("{OWNER}_wallet-coverage");
    start_clean(&pool).await;
    let chain = SyntheticChain::new(1000);
    chain.open_with_native(ETH);
    chain.open_with_token("0xobscure", "OBS", 6, 100_000_000);
    let prices = Arc::new(SyntheticPrices::new());
    prices.set("coingecko:ethereum", d("2600"));
    let known_before = connect(&pool, &account, &chain, &prices).await;

    // The obscure token has never been touched: it is not even followed, so the wallet can be valued.
    assert!(wallet::market(&pool, known_before.id, prices.clone()).await.is_ok());

    // Then the wallet sells it: a token moved on purpose and still without a price.
    chain.push_token(token_out("0xsold", 1010, "0xobscure", 40_000_000));
    chain.set_head(1100);
    later(&pool, &known_before, &chain, &prices, 2_000).await.unwrap();
    let error = match wallet::market(&pool, known_before.id, prices.clone()).await {
        Err(error) => error,
        Ok(_) => panic!("the figures should be refused"),
    };
    assert!(error.contains("ethereum:0xobscure"), "{error}");
    prices.set("ethereum:0xobscure", d("2"));
    assert!(wallet::market(&pool, known_before.id, prices.clone()).await.is_ok(), "once it can be priced the figures are back");
    cleanup(&pool, &account).await;
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see db_test.rs for how to start one."]
async fn an_error_from_a_network_never_carries_the_address_and_removing_the_account_removes_everything() {
    struct Failing;
    impl ReaderSource for Failing {
        fn reader(&self, _: &'static Chain) -> Result<Box<dyn ChainReader + Send>, String> {
            Err(format!("the explorer said: invalid address {WALLET} for this network"))
        }
    }
    let pool = pool().await;
    let account = format!("{OWNER}_wallet-private");
    start_clean(&pool).await;
    let chain = SyntheticChain::new(1000);
    let prices = Arc::new(SyntheticPrices::new());
    let connection = connect(&pool, &account, &chain, &prices).await;

    let error = wallet::sync(&pool, &connection, WALLET, &[ethereum()], Arc::new(Failing), prices.clone(), 2_000, true).await.unwrap_err();
    assert!(error.contains("[address]") && !error.to_lowercase().contains(&WALLET[2..]), "{error}");

    assert!(wallet::delete_connection(&pool, connection.id).await.unwrap());
    for table in ["wallet_chain_state", "wallet_effects", "wallet_anchors"] {
        let (left,): (i64,) = sqlx::query_as(&format!("SELECT count(*) FROM {table} WHERE connection_id = $1")).bind(connection.id).fetch_one(&pool).await.unwrap();
        assert_eq!(left, 0, "{table} goes with the connection");
    }
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see db_test.rs for how to start one."]
async fn rekey_also_covers_wallet_addresses() {
    use binance_worker::rekey;
    let pool = pool().await;
    let account = format!("{OWNER}_wallet-rekey");
    start_clean(&pool).await;
    let old = MasterKey::from_hex(&"33".repeat(32)).unwrap();
    wallet::insert_connection(&pool, &old, &account, WALLET, None).await.unwrap();
    let ring = MasterKey::from_hex(&"44".repeat(32)).unwrap().with_previous(&"33".repeat(32)).unwrap();
    rekey::rekey(&pool, &ring).await.unwrap();
    let moved = wallet::find_by_account(&pool, &account).await.unwrap().unwrap();
    let new_only = MasterKey::from_hex(&"44".repeat(32)).unwrap();
    assert_eq!(wallet::stored_address(&new_only, &moved).unwrap(), WALLET);
    cleanup(&pool, &account).await;
}

/// The chain's events happen at real moments after the connection, so the engine counts them.
fn receive_at(hash: &str, block: u64, wei: u128, time_ms: u64) -> NormalTx {
    NormalTx { time_ms, ..receive(hash, block, wei) }
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see db_test.rs for how to start one."]
async fn a_wallets_return_counts_the_price_move_and_not_the_money_it_received() {
    let pool = pool().await;
    let account = format!("{OWNER}_wallet-return");
    start_clean(&pool).await;
    let chain = SyntheticChain::new(1000);
    chain.open_with_native(ETH);
    let prices = Arc::new(SyntheticPrices::new());
    let connection = connect(&pool, &account, &chain, &prices).await;
    let connected = wallet::created_at_ms(&pool, connection.id).await.unwrap();
    let hour = 3_600_000u64;
    // Ether is $2,000 until a day and a half after connecting, $2,200 after.
    prices.set_schedule("coingecko:ethereum", &[(0, d("2000")), (connected + 36 * hour, d("2200"))]);

    // A day after connecting, another ether arrives (at $2,000): money in, not a gain.
    chain.push_normal(receive_at("0xdeposit", 1010, ETH, connected + 24 * hour));
    chain.set_head(1100);
    later(&pool, &connection, &chain, &prices, connected + 48 * hour).await.unwrap();

    let market = wallet::market(&pool, connection.id, prices.clone()).await.unwrap();
    let flows = wallet::flows(&pool, connection.id).await.unwrap();
    let history = binance_worker::history::history_from(Vec::new(), flows, connected, market, connected + 72 * hour).await.unwrap();
    let last = history.twr_index.last().unwrap().index;
    // 2 ether at $2,200 against 1 at $2,000 plus 1 deposited at $2,000: +10%, not +120%.
    assert!((last - d("1.10")).abs() < d("0.0005"), "the index ended at {last}");
    cleanup(&pool, &account).await;
}

#[tokio::test]
#[ignore = "requires a running local Postgres; see db_test.rs for how to start one."]
async fn swapping_one_asset_for_another_at_fair_prices_leaves_the_return_where_it_was() {
    let pool = pool().await;
    let account = format!("{OWNER}_wallet-swap");
    start_clean(&pool).await;
    let chain = SyntheticChain::new(1000);
    chain.open_with_token("0xusdc", "USDC", 6, 2_000_000_000);
    let prices = Arc::new(SyntheticPrices::new());
    prices.set("ethereum:0xusdc", d("1"));
    let connection = connect(&pool, &account, &chain, &prices).await;
    let connected = wallet::created_at_ms(&pool, connection.id).await.unwrap();
    let hour = 3_600_000u64;
    prices.set_schedule("coingecko:ethereum", &[(0, d("2000"))]);

    // 2,000 USDC become 1 ether at $2,000, with the ether leg arriving as an internal transfer.
    let swap_time = connected + 24 * hour;
    chain.push_token(TokenTransfer { time_ms: swap_time, ..token_out("0xswap", 1010, "0xusdc", 2_000_000_000) });
    chain.push_internal(InternalTx { hash: "0xswap".into(), block: 1010, time_ms: swap_time, from: OTHER.into(), to: WALLET.into(), value: ETH.to_string(), index: "1".into(), failed: false });
    chain.set_head(1100);
    later(&pool, &connection, &chain, &prices, connected + 48 * hour).await.unwrap();

    let flows = wallet::flows(&pool, connection.id).await.unwrap();
    assert_eq!(flows.iter().map(|f| f.source_namespace).collect::<Vec<_>>(), vec![FlowFamily::Convert]);
    let market = wallet::market(&pool, connection.id, prices.clone()).await.unwrap();
    let history = binance_worker::history::history_from(Vec::new(), flows, connected, market, connected + 72 * hour).await.unwrap();
    let last = history.twr_index.last().unwrap().index;
    assert!((last - d("1")).abs() < d("0.0005"), "a fair swap changes no value: the index ended at {last}");
    cleanup(&pool, &account).await;
}
