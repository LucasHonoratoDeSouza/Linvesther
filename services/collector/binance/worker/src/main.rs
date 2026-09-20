//! CLI entry point.
//!
//! ```text
//! binance-worker connect    # reads {"accountId","apiKey","apiSecret","symbols"} JSON from stdin,
//!                            # validates+stores the credential, runs one sync cycle, prints a JSON summary
//! binance-worker status     # reads {"accountId"} JSON from stdin, prints connection status as JSON
//! binance-worker nav        # reads {"accountId"} JSON from stdin, computes and prints a real current NAV snapshot
//! binance-worker performance # reads {"accountId"} JSON from stdin, prints real daily NAV/returns plus
//!                             # Sharpe/Sortino/MDD/CAGR/win-rate (each independently null when insufficient sample)
//! binance-worker prove-performance # reads {"accountId"} JSON from stdin, runs REAL RISC Zero proving
//!                                   # (CPU/RAM-heavy) over the account's real, collected history and prints
//!                                   # a portable, independently-verifiable proof of return/max-drawdown —
//!                                   # never the raw trades, flows, balance or account identifier
//! binance-worker scheduler  # long-running: syncs every active connection on an interval
//! ```
//!
//! Never accepts a credential via a CLI argument (arguments are
//! visible in the process list to anyone on the same machine) — always
//! stdin. `DATABASE_URL`, `BINANCE_WORKER_ENCRYPTION_KEY` and (for
//! `prove-performance`) `BINANCE_WORKER_A0_SIGNING_KEY` come from the
//! environment.

use binance_worker::crypto::{decrypt, encrypt, MasterKey};
use binance_worker::{coinbase, db, ibkr, stream, sync};
use coinbase_client::{CoinbaseClient, Credentials};
use ibkr_client::{Credentials as IbkrCredentials, IbkrClient};
use exchange_core::MarketData;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use binance_worker::market::BinanceMarket;
use sqlx::postgres::PgPoolOptions;
use std::collections::HashMap;
use std::io::Read;
use uuid::Uuid;

#[derive(Deserialize)]
struct ConnectRequest {
    #[serde(rename = "accountId")]
    account_id: String,
    /// "binance" (default) or "coinbase".
    #[serde(default)]
    exchange: Option<String>,
    #[serde(rename = "apiKey", default)]
    api_key: String,
    #[serde(rename = "apiSecret", default)]
    api_secret: String,
    /// Coinbase: the CDP key's name and its EC private key (PEM).
    #[serde(rename = "keyName", default)]
    key_name: String,
    #[serde(rename = "privateKey", default)]
    private_key: String,
    /// IBKR: the Flex Web Service token and the Flex Query ID, both
    /// read-only, from Account Management → Reporting → Flex Queries.
    #[serde(default)]
    token: String,
    #[serde(rename = "queryId", default)]
    query_id: String,
    /// Optional name the person gives this connected account.
    #[serde(default)]
    label: Option<String>,
    /// Optional: specific symbols to backfill trade *history* for at
    /// connect time (myTrades requires a symbol per call, so there is
    /// no way to backfill "everything ever traded" without knowing
    /// which pairs to ask about). Defaults to none — ongoing trades on
    /// any symbol are still captured in real time via the account's
    /// WebSocket user data stream (see `stream.rs`), no selection
    /// needed for that part.
    #[serde(default)]
    symbols: Vec<String>,
}

#[derive(Deserialize)]
struct StatusRequest {
    #[serde(rename = "accountId")]
    account_id: String,
}

fn read_stdin_json<T: for<'a> Deserialize<'a>>() -> Result<T, String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&buf).map_err(|e| e.to_string())
}

fn master_key() -> Result<MasterKey, String> {
    let hex_key = binance_worker::secrets::read_secret("BINANCE_WORKER_ENCRYPTION_KEY")?.ok_or_else(|| {
        "BINANCE_WORKER_ENCRYPTION_KEY (or BINANCE_WORKER_ENCRYPTION_KEY_FILE) must be set (generate with: openssl rand -hex 32)"
            .to_string()
    })?;
    MasterKey::from_hex(&hex_key).map_err(|e| e.to_string())
}

/// The collector's own A0 signing key (per the protocol specification:
/// "A0 é assinatura de coletor identificado") — this worker signs as
/// the identified collector of what it gathered from Binance, not as
/// the exchange itself. Separate from `BINANCE_WORKER_ENCRYPTION_KEY`
/// (that one protects stored credentials at rest; this one is the
/// public-verifiable signing identity behind every proof this worker
/// produces).
#[cfg(feature = "proving")]
fn a0_signing_key() -> Result<[u8; 32], String> {
    let hex_key = binance_worker::secrets::read_secret("BINANCE_WORKER_A0_SIGNING_KEY")?.ok_or_else(|| {
        "BINANCE_WORKER_A0_SIGNING_KEY (or BINANCE_WORKER_A0_SIGNING_KEY_FILE) must be set (generate with: openssl rand -hex 32)"
            .to_string()
    })?;
    let bytes = hex::decode(&hex_key).map_err(|e| e.to_string())?;
    bytes
        .try_into()
        .map_err(|_| "BINANCE_WORKER_A0_SIGNING_KEY must be exactly 32 bytes (64 hex chars)".to_string())
}

#[derive(Serialize)]
struct CollectorIdentityResponse {
    /// The fingerprint of this worker's A0 signing key, or `null` when no
    /// key is configured (so it cannot back a proof).
    #[serde(rename = "fingerprintHex")]
    fingerprint_hex: Option<String>,
}

/// Which collector this worker is. Never exposes the key itself — only the
/// fingerprint a verifier looks up in its list of trusted collectors.
fn run_collector_identity() -> Result<CollectorIdentityResponse, String> {
    #[cfg(feature = "proving")]
    let fingerprint_hex = match binance_worker::secrets::read_secret("BINANCE_WORKER_A0_SIGNING_KEY")? {
        Some(_) => {
            let bytes = a0_signing_key()?;
            let signer = collector_attestation::signer::A0Signer::from_bytes(&bytes).map_err(|e| e.to_string())?;
            Some(hex::encode(signer.fingerprint()))
        }
        None => None,
    };
    #[cfg(not(feature = "proving"))]
    let fingerprint_hex = None;
    Ok(CollectorIdentityResponse { fingerprint_hex })
}

/// A read-only-verified Binance market connection. Blocking HTTP, so
/// callers run it under `spawn_blocking` (see sync.rs's fetch_blocking).
fn binance_market(api_key: String, api_secret: String) -> Result<Arc<dyn MarketData>, String> {
    let mut client = binance_live_client::LiveClient::new(
        api_key,
        api_secret,
        collector_credentials::Environment::Production,
    );
    client.ensure_read_only().map_err(|e| e.to_string())?;
    Ok(Arc::new(BinanceMarket::new(client)))
}

async fn open_binance_market(api_key: String, api_secret: String) -> Result<Arc<dyn MarketData>, String> {
    tokio::task::spawn_blocking(move || binance_market(api_key, api_secret))
        .await
        .map_err(|e| e.to_string())?
}

async fn pool() -> Result<sqlx::PgPool, String> {
    let url = std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must be set".to_string())?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .map_err(|e| e.to_string())?;
    db::run_migrations(&pool).await.map_err(|e| e.to_string())?;
    Ok(pool)
}

#[derive(Serialize)]
struct ConnectResponse {
    #[serde(rename = "connectionId")]
    connection_id: String,
    summary: SyncSummaryJson,
}

#[derive(Serialize)]
struct SyncSummaryJson {
    #[serde(rename = "tradesFetched")]
    trades_fetched: usize,
    #[serde(rename = "flowsFetched")]
    flows_fetched: usize,
    #[serde(rename = "catalogSymbolsFetched")]
    catalog_symbols_fetched: usize,
}

/// A connected account, whichever exchange it is on.
enum Account {
    Binance(db::StoredConnection),
    Coinbase(coinbase::CoinbaseConnection),
    Ibkr(ibkr::IbkrConnection),
}

async fn resolve(pool: &sqlx::PgPool, account_id: &str) -> Result<Account, String> {
    if let Some(c) = db::find_connection_by_account(pool, account_id).await.map_err(|e| e.to_string())? {
        return Ok(Account::Binance(c));
    }
    if let Some(c) = coinbase::find_by_account(pool, account_id).await.map_err(|e| e.to_string())? {
        return Ok(Account::Coinbase(c));
    }
    if let Some(c) = ibkr::find_by_account(pool, account_id).await.map_err(|e| e.to_string())? {
        return Ok(Account::Ibkr(c));
    }
    Err(format!("no connection for account {account_id}"))
}

async fn open_coinbase(credentials: Credentials) -> Result<Arc<CoinbaseClient>, String> {
    tokio::task::spawn_blocking(move || {
        let client = CoinbaseClient::new(credentials);
        client.ensure_read_only().map_err(|e| e.to_string())?;
        Ok::<_, String>(Arc::new(client))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// `reqwest::blocking::Client` must be built, used and dropped entirely
/// within a blocking context — building it directly on the async
/// executor thread and only later dropping it inside `spawn_blocking`
/// (as happened before this fix) panics ("Cannot drop a runtime in a
/// context where blocking is not allowed").
async fn open_ibkr(credentials: IbkrCredentials) -> Result<Arc<IbkrClient>, String> {
    tokio::task::spawn_blocking(move || Arc::new(IbkrClient::new(credentials)))
        .await
        .map_err(|e| e.to_string())
}

async fn ibkr_summary(pool: &sqlx::PgPool, c: &ibkr::IbkrConnection) -> Result<SyncSummaryJson, String> {
    Ok(SyncSummaryJson {
        trades_fetched: ibkr::stored_equity_count(pool, c.id).await.map_err(|e| e.to_string())? as usize,
        flows_fetched: ibkr::stored_flow_count(pool, c.id).await.map_err(|e| e.to_string())? as usize,
        catalog_symbols_fetched: 0,
    })
}

/// Market access for the account, read-only verified. For Coinbase the
/// stored fills and transfers are brought up to date first, since its
/// history is built from what was collected since connecting.
async fn open_market(pool: &sqlx::PgPool, key: &MasterKey, account: &Account, now_ms: u64) -> Result<Arc<dyn MarketData>, String> {
    match account {
        Account::Binance(c) => {
            let (api_key, api_secret) = decrypt_connection_credential(key, c).ok_or("could not decrypt stored credential")?;
            open_binance_market(api_key, api_secret).await
        }
        Account::Coinbase(c) => {
            let client = open_coinbase(coinbase::credentials(key, c)?).await?;
            coinbase::sync(pool, c, client.clone(), now_ms).await?;
            Ok(client)
        }
        Account::Ibkr(_) => unreachable!("callers handle Ibkr before reaching open_market"),
    }
}

fn now_ms() -> Result<u64, String> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis() as u64)
}

async fn run_history(pool: &sqlx::PgPool, account: &Account, client: Arc<dyn MarketData>, now: u64) -> Result<binance_worker::history::HistoryResult, String> {
    match account {
        Account::Binance(c) => binance_worker::history::load_and_compute_history(pool, c.id, client, now).await.map_err(|e| e.to_string()),
        Account::Coinbase(c) => {
            let trades = coinbase::trades(pool, c.id).await.map_err(|e| e.to_string())?;
            let flows = coinbase::flows(pool, c.id).await.map_err(|e| e.to_string())?;
            let since = coinbase::created_at_ms(pool, c.id).await.map_err(|e| e.to_string())?;
            binance_worker::history::history_from(trades, flows, since, client, now).await.map_err(|e| e.to_string())
        }
        Account::Ibkr(_) => unreachable!("callers handle Ibkr before reaching run_history"),
    }
}

async fn since_ms(pool: &sqlx::PgPool, account: &Account) -> Result<u64, String> {
    match account {
        Account::Binance(c) => db::connection_created_at_ms(pool, c.id).await.map_err(|e| e.to_string()),
        Account::Coinbase(c) => coinbase::created_at_ms(pool, c.id).await.map_err(|e| e.to_string()),
        Account::Ibkr(c) => ibkr::created_at_ms(pool, c.id).await.map_err(|e| e.to_string()),
    }
}

async fn coinbase_summary(pool: &sqlx::PgPool, c: &coinbase::CoinbaseConnection) -> Result<SyncSummaryJson, String> {
    Ok(SyncSummaryJson {
        trades_fetched: coinbase::stored_fill_count(pool, c.id).await.map_err(|e| e.to_string())? as usize,
        flows_fetched: coinbase::stored_flow_count(pool, c.id).await.map_err(|e| e.to_string())? as usize,
        catalog_symbols_fetched: 0,
    })
}

async fn run_connect() -> Result<ConnectResponse, String> {
    let request: ConnectRequest = read_stdin_json()?;
    let key = master_key()?;
    let pool = pool().await?;
    let label = request.label.as_deref().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string);

    if request.exchange.as_deref() == Some("ibkr") {
        let credentials = IbkrCredentials::new(&request.token, &request.query_id).map_err(|e| e.to_string())?;
        let client = open_ibkr(credentials).await?;
        let stored_token = encrypt(&key, &request.token).map_err(|e| e.to_string())?;
        let stored_query = encrypt(&key, &request.query_id).map_err(|e| e.to_string())?;
        let id = ibkr::insert_connection(&pool, &request.account_id, &stored_token, &stored_query)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(label) = &label {
            ibkr::set_label(&pool, id, label).await.map_err(|e| e.to_string())?;
        }
        let connection = ibkr::find_by_account(&pool, &request.account_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("connection vanished")?;
        ibkr::sync(&pool, &connection, client, now_ms()?).await?;
        return Ok(ConnectResponse { connection_id: id.to_string(), summary: ibkr_summary(&pool, &connection).await? });
    }

    if request.exchange.as_deref() == Some("coinbase") {
        let credentials = Credentials::new(request.key_name.trim(), &request.private_key).map_err(|e| e.to_string())?;
        let client = open_coinbase(credentials).await?;
        let stored_name = encrypt(&key, request.key_name.trim()).map_err(|e| e.to_string())?;
        let stored_key = encrypt(&key, &request.private_key).map_err(|e| e.to_string())?;
        let id = coinbase::insert_connection(&pool, &request.account_id, &stored_name, &stored_key)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(label) = &label {
            coinbase::set_label(&pool, id, label).await.map_err(|e| e.to_string())?;
        }
        let connection = coinbase::find_by_account(&pool, &request.account_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("connection vanished")?;
        coinbase::sync(&pool, &connection, client, now_ms()?).await?;
        return Ok(ConnectResponse { connection_id: id.to_string(), summary: coinbase_summary(&pool, &connection).await? });
    }

    let encrypted_key = encrypt(&key, &request.api_key).map_err(|e| e.to_string())?;
    let encrypted_secret = encrypt(&key, &request.api_secret).map_err(|e| e.to_string())?;

    let connection_id = db::insert_connection(
        &pool,
        &request.account_id,
        &encrypted_key,
        &encrypted_secret,
        &request.symbols,
    )
    .await
    .map_err(|e| e.to_string())?;

    if let Some(label) = request.label.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        db::set_connection_label(&pool, connection_id, label)
            .await
            .map_err(|e| e.to_string())?;
    }

    let summary = sync::sync_and_persist(
        &pool,
        connection_id,
        request.api_key,
        request.api_secret,
        request.symbols,
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(ConnectResponse {
        connection_id: connection_id.to_string(),
        summary: SyncSummaryJson {
            trades_fetched: summary.trades_fetched,
            flows_fetched: summary.flows_fetched,
            catalog_symbols_fetched: summary.catalog_symbols_fetched,
        },
    })
}

/// Pulls the connection's deposits/withdrawals (and refreshes the market
/// catalog) from the exchange right now, instead of waiting for the
/// scheduler's next periodic pass.
async fn run_sync() -> Result<SyncSummaryJson, String> {
    let request: StatusRequest = read_stdin_json()?;
    let key = master_key()?;
    let pool = pool().await?;
    match resolve(&pool, &request.account_id).await? {
        Account::Binance(connection) => {
            let (api_key, api_secret) = decrypt_connection_credential(&key, &connection)
                .ok_or("could not decrypt stored credential")?;
            let summary = sync::sync_and_persist(&pool, connection.id, api_key, api_secret, connection.symbols)
                .await
                .map_err(|e| e.to_string())?;
            Ok(SyncSummaryJson {
                trades_fetched: summary.trades_fetched,
                flows_fetched: summary.flows_fetched,
                catalog_symbols_fetched: summary.catalog_symbols_fetched,
            })
        }
        Account::Coinbase(connection) => {
            let client = open_coinbase(coinbase::credentials(&key, &connection)?).await?;
            coinbase::sync(&pool, &connection, client, now_ms()?).await?;
            coinbase_summary(&pool, &connection).await
        }
        Account::Ibkr(connection) => {
            let client = open_ibkr(ibkr::credentials(&key, &connection)?).await?;
            ibkr::sync(&pool, &connection, client, now_ms()?).await?;
            ibkr_summary(&pool, &connection).await
        }
    }
}

#[derive(Deserialize)]
struct SeriesRequest {
    #[serde(rename = "accountId")]
    account_id: String,
    /// "24h" | "7d" | "30d" | "1y" | "5y" | "max"
    range: String,
}

#[derive(Serialize)]
struct SeriesPointJson {
    #[serde(rename = "timeMs")]
    time_ms: u64,
    nav: String,
    /// Time-weighted return index, 1 at the first point; deposits and
    /// withdrawals never move it.
    index: String,
}

#[derive(Serialize)]
struct SeriesResponse {
    points: Vec<SeriesPointJson>,
    #[serde(rename = "stepMs")]
    step_ms: u64,
    /// When the account was connected — no range reaches back before it.
    #[serde(rename = "sinceMs")]
    since_ms: u64,
}

async fn run_series() -> Result<SeriesResponse, String> {
    let request: SeriesRequest = read_stdin_json()?;
    let range = binance_worker::history::SeriesRange::parse(&request.range)
        .ok_or_else(|| format!("unknown range {}", request.range))?;
    let key = master_key()?;
    let pool = pool().await?;
    let account = resolve(&pool, &request.account_id).await?;

    if let Account::Ibkr(c) = &account {
        let since_ms = ibkr::created_at_ms(&pool, c.id).await.map_err(|e| e.to_string())?;
        let series = ibkr::load_series(&pool, c.id, range).await?;
        return Ok(SeriesResponse {
            points: series
                .points
                .iter()
                .map(|p| SeriesPointJson { time_ms: p.time_ms, nav: p.nav.to_string(), index: p.index.to_string() })
                .collect(),
            step_ms: series.step_ms,
            since_ms,
        });
    }

    let now = now_ms()?;
    let client = open_market(&pool, &key, &account, now).await?;
    let since_ms = since_ms(&pool, &account).await?;
    let series = match &account {
        Account::Binance(c) => binance_worker::history::load_and_compute_series(&pool, c.id, client, now, range).await,
        Account::Coinbase(c) => {
            let trades = coinbase::trades(&pool, c.id).await.map_err(|e| e.to_string())?;
            let flows = coinbase::flows(&pool, c.id).await.map_err(|e| e.to_string())?;
            binance_worker::history::series_from(trades, flows, since_ms, client, now, range).await
        }
        Account::Ibkr(_) => unreachable!("handled above"),
    }
    .map_err(|e| e.to_string())?;
    Ok(SeriesResponse {
        points: series
            .points
            .iter()
            .map(|p| SeriesPointJson {
                time_ms: p.time_ms,
                nav: p.nav.to_string(),
                index: p.index.to_string(),
            })
            .collect(),
        step_ms: series.step_ms,
        since_ms,
    })
}

#[derive(Deserialize)]
struct ListRequest {
    #[serde(rename = "ownerAddress")]
    owner_address: String,
}

#[derive(Serialize)]
struct ListResponse {
    accounts: Vec<db::OwnedConnection>,
}

async fn run_list() -> Result<ListResponse, String> {
    let request: ListRequest = read_stdin_json()?;
    let pool = pool().await?;
    let mut accounts = db::connections_owned_by(&pool, &request.owner_address)
        .await
        .map_err(|e| e.to_string())?;
    accounts.extend(coinbase::owned_by(&pool, &request.owner_address).await.map_err(|e| e.to_string())?);
    accounts.extend(ibkr::owned_by(&pool, &request.owner_address).await.map_err(|e| e.to_string())?);
    accounts.sort_by_key(|a| a.connected_at_ms);
    Ok(ListResponse { accounts })
}

#[derive(Deserialize)]
struct RenameRequest {
    #[serde(rename = "accountId")]
    account_id: String,
    label: String,
}

#[derive(Serialize)]
struct RenameResponse {
    label: String,
}

async fn run_rename() -> Result<RenameResponse, String> {
    let request: RenameRequest = read_stdin_json()?;
    let label = request.label.trim().to_string();
    if label.is_empty() {
        return Err("the name can't be empty".to_string());
    }
    let pool = pool().await?;
    match resolve(&pool, &request.account_id).await? {
        Account::Binance(c) => db::set_connection_label(&pool, c.id, &label).await,
        Account::Coinbase(c) => coinbase::set_label(&pool, c.id, &label).await,
        Account::Ibkr(c) => ibkr::set_label(&pool, c.id, &label).await,
    }
    .map_err(|e| e.to_string())?;
    Ok(RenameResponse { label })
}

async fn run_status() -> Result<db::ConnectionSummary, String> {
    let request: StatusRequest = read_stdin_json()?;
    let pool = pool().await?;
    if let Some(c) = coinbase::find_by_account(&pool, &request.account_id).await.map_err(|e| e.to_string())? {
        let counts = coinbase_summary(&pool, &c).await?;
        return Ok(db::ConnectionSummary {
            connected: true,
            status: Some(c.status),
            last_synced_at: c.last_synced_at,
            trade_count: counts.trades_fetched as i64,
            flow_count: counts.flows_fetched as i64,
        });
    }
    if let Some(c) = ibkr::find_by_account(&pool, &request.account_id).await.map_err(|e| e.to_string())? {
        let counts = ibkr_summary(&pool, &c).await?;
        return Ok(db::ConnectionSummary {
            connected: true,
            status: Some(c.status),
            last_synced_at: c.last_synced_at,
            trade_count: counts.trades_fetched as i64,
            flow_count: counts.flows_fetched as i64,
        });
    }
    db::connection_summary(&pool, &request.account_id)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct NavAssetJson {
    asset: String,
    quantity: String,
    price: String,
    value: String,
}

#[derive(Serialize)]
struct NavResponse {
    nav: String,
    currency: String,
    assets: Vec<NavAssetJson>,
    #[serde(rename = "excludedOutOfScopeAssets")]
    excluded_out_of_scope_assets: Vec<String>,
}

/// Reads the connection's stored, encrypted credential and computes a
/// real, current NAV snapshot — see `publish.rs` for what this does
/// and does not do (no historical series yet, only "right now").
async fn run_nav() -> Result<NavResponse, String> {
    let request: StatusRequest = read_stdin_json()?;
    let key = master_key()?;
    let pool = pool().await?;
    let account = resolve(&pool, &request.account_id).await?;

    if let Account::Ibkr(c) = &account {
        let (nav, currency) = ibkr::current_nav(&pool, c.id).await?.ok_or("no data synced yet for this account")?;
        return Ok(NavResponse { nav: nav.to_string(), currency, assets: Vec::new(), excluded_out_of_scope_assets: Vec::new() });
    }

    let client = match &account {
        Account::Binance(_) => open_market(&pool, &key, &account, 0).await?,
        Account::Coinbase(c) => open_coinbase(coinbase::credentials(&key, c)?).await?,
        Account::Ibkr(_) => unreachable!("handled above"),
    };
    let quote_currency = client.quote_currency().to_string();
    let result = tokio::task::spawn_blocking(move || binance_worker::publish::compute_current_nav(client.as_ref()))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    Ok(NavResponse {
        nav: result.valuation.nav.to_string(),
        currency: quote_currency,
        assets: result
            .valuation
            .assets
            .iter()
            .map(|a| NavAssetJson {
                asset: a.asset.clone(),
                quantity: a.quantity.to_string(),
                price: a.price.to_string(),
                value: a.value.to_string(),
            })
            .collect(),
        excluded_out_of_scope_assets: result.excluded_out_of_scope_assets,
    })
}

#[derive(Serialize)]
struct DailyNavJson {
    #[serde(rename = "dayStartMs")]
    day_start_ms: u64,
    nav: String,
}

#[derive(Serialize)]
struct WinRateJson {
    #[serde(rename = "closedRoundTrips")]
    closed_round_trips: usize,
    wins: usize,
    #[serde(rename = "winRate")]
    win_rate: String,
}

#[derive(Serialize)]
struct PositionPnlJson {
    asset: String,
    quantity: String,
    #[serde(rename = "averageCost")]
    average_cost: Option<String>,
    #[serde(rename = "markPrice")]
    mark_price: String,
    realized: String,
    unrealized: Option<String>,
}

/// Profit since the account was connected, in USDT — see `pnl.rs`.
#[derive(Serialize)]
struct PnlJson {
    positions: Vec<PositionPnlJson>,
    realized: String,
    unrealized: String,
    fees: String,
}

/// A metric that may not be computable yet (insufficient sample) is
/// `null`, paired with a human-readable `*Unavailable` reason string —
/// never a fabricated number standing in for "not enough history."
#[derive(Serialize)]
struct PerformanceResponse {
    #[serde(rename = "dailyNav")]
    daily_nav: Vec<DailyNavJson>,
    #[serde(rename = "dailyReturns")]
    daily_returns: Vec<String>,
    sharpe: Option<String>,
    #[serde(rename = "sharpeUnavailable")]
    sharpe_unavailable: Option<String>,
    sortino: Option<String>,
    #[serde(rename = "sortinoUnavailable")]
    sortino_unavailable: Option<String>,
    #[serde(rename = "maxDrawdown")]
    max_drawdown: Option<String>,
    cagr: Option<String>,
    #[serde(rename = "cagrUnavailable")]
    cagr_unavailable: Option<String>,
    #[serde(rename = "winRate")]
    win_rate: Option<WinRateJson>,
    #[serde(rename = "earliestReliableCheckpointMs")]
    earliest_reliable_checkpoint_ms: Option<u64>,
    pnl: PnlJson,
}

async fn run_performance() -> Result<PerformanceResponse, String> {
    let request: StatusRequest = read_stdin_json()?;
    let key = master_key()?;
    let pool = pool().await?;
    let account = resolve(&pool, &request.account_id).await?;

    if let Account::Ibkr(c) = &account {
        let performance = ibkr::load_performance(&pool, c.id).await?;
        return Ok(PerformanceResponse {
            daily_nav: performance.daily_nav.iter().map(|(t, nav)| DailyNavJson { day_start_ms: *t, nav: nav.to_string() }).collect(),
            daily_returns: performance.daily_returns.iter().map(|r| r.to_string()).collect(),
            sharpe: performance.sharpe.as_ref().ok().map(|v| v.to_string()),
            sharpe_unavailable: performance.sharpe.as_ref().err().map(|e| e.to_string()),
            sortino: performance.sortino.as_ref().ok().map(|v| v.to_string()),
            sortino_unavailable: performance.sortino.as_ref().err().map(|e| e.to_string()),
            max_drawdown: performance.max_drawdown.as_ref().map(|episode| episode.magnitude.to_string()),
            cagr: performance.cagr.as_ref().ok().map(|v| v.to_string()),
            cagr_unavailable: performance.cagr.as_ref().err().map(|e| e.to_string()),
            win_rate: performance.win_rate.map(|w| WinRateJson { closed_round_trips: w.closed_round_trips, wins: w.wins, win_rate: w.win_rate.to_string() }),
            earliest_reliable_checkpoint_ms: performance.earliest_reliable_checkpoint_ms,
            // IBKR gives one total change since connection, not a
            // realized/unrealized split or per-position detail — see
            // ibkr.rs's own doc comment.
            pnl: PnlJson {
                positions: Vec::new(),
                realized: "0".to_string(),
                unrealized: performance.total_change.to_string(),
                fees: "0".to_string(),
            },
        });
    }

    let now = now_ms()?;
    let client = open_market(&pool, &key, &account, now).await?;
    let history = run_history(&pool, &account, client, now).await?;
    let summary = binance_worker::history::summarize_performance(history);

    Ok(PerformanceResponse {
        daily_nav: summary
            .daily_nav
            .iter()
            .map(|p| DailyNavJson {
                day_start_ms: p.day_start_ms,
                nav: p.nav.to_string(),
            })
            .collect(),
        daily_returns: summary
            .daily_returns
            .iter()
            .map(|r| r.to_string())
            .collect(),
        sharpe: summary.sharpe.as_ref().ok().map(|v| v.to_string()),
        sharpe_unavailable: summary.sharpe.as_ref().err().map(|e| e.to_string()),
        sortino: summary.sortino.as_ref().ok().map(|v| v.to_string()),
        sortino_unavailable: summary.sortino.as_ref().err().map(|e| e.to_string()),
        max_drawdown: summary
            .max_drawdown
            .as_ref()
            .map(|episode| episode.magnitude.to_string()),
        cagr: summary.cagr.as_ref().ok().map(|v| v.to_string()),
        cagr_unavailable: summary.cagr.as_ref().err().map(|e| e.to_string()),
        win_rate: summary.win_rate.map(|w| WinRateJson {
            closed_round_trips: w.closed_round_trips,
            wins: w.wins,
            win_rate: w.win_rate.to_string(),
        }),
        earliest_reliable_checkpoint_ms: summary.earliest_reliable_checkpoint_ms,
        pnl: PnlJson {
            positions: summary
                .pnl
                .positions
                .iter()
                .map(|p| PositionPnlJson {
                    asset: p.asset.clone(),
                    quantity: p.quantity.to_string(),
                    average_cost: p.average_cost.map(|v| v.to_string()),
                    mark_price: p.mark_price.to_string(),
                    realized: p.realized.to_string(),
                    unrealized: p.unrealized.map(|v| v.to_string()),
                })
                .collect(),
            realized: summary.pnl.realized.to_string(),
            unrealized: summary.pnl.unrealized.to_string(),
            fees: summary.pnl.fees.to_string(),
        },
    })
}

#[cfg(feature = "proving")]
#[derive(Serialize)]
struct ProvePerformanceResponse {
    #[serde(rename = "envelopeDigestHex")]
    envelope_digest_hex: String,
    /// Which collector key signed the source data (also committed in the
    /// journal). Compare it against a list of collectors you trust.
    #[serde(rename = "signerFingerprintHex")]
    signer_fingerprint_hex: String,
    #[serde(rename = "periodStartMs")]
    period_start_ms: i64,
    #[serde(rename = "periodEndMs")]
    period_end_ms: i64,
    /// The proven return over the period, as a decimal string (e.g.
    /// `"0.042000"` for +4.2%) — derived from the guest's own
    /// `twr_index_scaled` journal field, never recomputed off-chain.
    #[serde(rename = "returnFraction")]
    return_fraction: String,
    #[serde(rename = "maxDrawdownBp")]
    max_drawdown_bp: i64,
    #[serde(rename = "imageIdHex")]
    image_id_hex: String,
    /// `bincode`-encoded receipt, hex-encoded — the exact format
    /// `crates/verifier::receipt::verify_receipt` and
    /// `linvesther-verify` already expect (`receiptHex` in a bundle's
    /// `proofs/index.json`), so this can be verified independently
    /// without trusting this API or worker again.
    #[serde(rename = "receiptHex")]
    receipt_hex: String,
    #[serde(rename = "journalHex")]
    journal_hex: String,
}

#[cfg(feature = "proving")]
/// Runs real RISC Zero proving over this connection's real, collected
/// history and returns a portable, independently-verifiable proof of
/// its return/max-drawdown over the covered period — never the raw
/// trades, flows, balance or account identifier (those stay behind the
/// envelope's commitments; see `prove.rs`'s own doc comment for why the
/// series fed to the guest is the flow-adjusted TWR index, not raw
/// NAV). This is CPU/RAM-heavy real proving, not a stub — callers
/// should warn before invoking it.
async fn run_prove_performance() -> Result<ProvePerformanceResponse, String> {
    let request: StatusRequest = read_stdin_json()?;
    let key = master_key()?;
    let signing_key = a0_signing_key()?;
    let pool = pool().await?;
    let connection = db::find_connection_by_account(&pool, &request.account_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no connection for account {}", request.account_id))?;
    let (api_key, api_secret) = decrypt_connection_credential(&key, &connection)
        .ok_or("could not decrypt stored credential")?;

    let client = open_binance_market(api_key, api_secret).await?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis() as u64;

    let proof = binance_worker::prove::prove_performance(
        &pool,
        connection.id,
        &request.account_id,
        client,
        &signing_key,
        now_ms,
    )
    .await
    .map_err(|e| e.to_string())?;

    let return_fraction = rust_decimal::Decimal::from_i128_with_scale(proof.twr_return_scaled, 6);
    let mut image_id_bytes = Vec::with_capacity(32);
    for word in proof.image_id {
        image_id_bytes.extend_from_slice(&word.to_le_bytes());
    }
    let receipt_bytes = bincode::serialize(&proof.receipt).map_err(|e| e.to_string())?;

    Ok(ProvePerformanceResponse {
        envelope_digest_hex: hex::encode(proof.envelope_digest),
        signer_fingerprint_hex: hex::encode(proof.signer_fingerprint),
        period_start_ms: proof.period_start_ms,
        period_end_ms: proof.period_end_ms,
        return_fraction: return_fraction.to_string(),
        max_drawdown_bp: proof.mdd_bp,
        image_id_hex: hex::encode(image_id_bytes),
        receipt_hex: hex::encode(receipt_bytes),
        journal_hex: hex::encode(&proof.receipt.journal.bytes),
    })
}

fn decrypt_connection_credential(
    key: &MasterKey,
    connection: &db::StoredConnection,
) -> Option<(String, String)> {
    let nonce_key: [u8; 12] = connection.nonce_api_key.clone().try_into().ok()?;
    let nonce_secret: [u8; 12] = connection.nonce_api_secret.clone().try_into().ok()?;
    let api_key = decrypt(key, &connection.encrypted_api_key, &nonce_key).ok()?;
    let api_secret = decrypt(key, &connection.encrypted_api_secret, &nonce_secret).ok()?;
    Some((api_key, api_secret))
}

/// Ensures every currently-active connection has a live trade-stream
/// task running (see `stream.rs`) — the real-time, no-symbol-selection
/// path for new fills. A task that already finished (the connection
/// dropped, or the key stopped being read-only) is respawned on the
/// next tick; this loop's own interval is the reconnect backoff, so a
/// persistently failing connection retries steadily rather than in a
/// tight loop.
async fn ensure_trade_streams_running(
    pool: &sqlx::PgPool,
    key: &MasterKey,
    tasks: &mut HashMap<Uuid, tokio::task::JoinHandle<()>>,
) {
    // interval_seconds=0 here means "every active connection", not
    // "due for the periodic HTTP sync" — trade streaming runs
    // independently of that interval.
    let active = match db::connections_due_for_sync(pool, 0).await {
        Ok(connections) => connections,
        Err(e) => {
            eprintln!("could not list connections for stream management: {e}");
            return;
        }
    };
    for connection in active {
        let still_running = tasks
            .get(&connection.id)
            .is_some_and(|handle| !handle.is_finished());
        if still_running {
            continue;
        }
        let Some((api_key, api_secret)) = decrypt_connection_credential(key, &connection) else {
            eprintln!(
                "skip stream for {}: could not decrypt credential",
                connection.account_id
            );
            continue;
        };
        let pool = pool.clone();
        let account_id = connection.account_id.clone();
        let connection_id = connection.id;
        let handle = tokio::spawn(async move {
            if let Err(e) =
                stream::run_trade_stream(&pool, connection_id, &api_key, &api_secret).await
            {
                eprintln!("trade stream for {account_id} ended: {e}");
            }
        });
        tasks.insert(connection.id, handle);
    }
}

async fn run_scheduler() -> Result<(), String> {
    let key = master_key()?;
    let pool = pool().await?;
    let interval_seconds: i64 = std::env::var("BINANCE_WORKER_SYNC_INTERVAL_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(900); // 15 minutes by default; governs deposit/withdrawal/catalog polling only — trades are real-time (see stream.rs).

    eprintln!("binance-worker scheduler starting, interval={interval_seconds}s");
    let mut stream_tasks: HashMap<Uuid, tokio::task::JoinHandle<()>> = HashMap::new();
    loop {
        ensure_trade_streams_running(&pool, &key, &mut stream_tasks).await;

        match db::connections_due_for_sync(&pool, interval_seconds).await {
            Ok(due) => {
                for connection in due {
                    let connection_id = connection.id;
                    let account_id = connection.account_id.clone();
                    let Some((api_key, api_secret)) =
                        decrypt_connection_credential(&key, &connection)
                    else {
                        eprintln!("skip {account_id}: could not decrypt credential");
                        continue;
                    };
                    match sync::sync_and_persist(
                        &pool,
                        connection_id,
                        api_key,
                        api_secret,
                        connection.symbols,
                    )
                    .await
                    {
                        Ok(summary) => eprintln!("synced {account_id}: {summary:?}"),
                        Err(e) => eprintln!("sync failed for {account_id}: {e}"),
                    }
                }
            }
            Err(e) => eprintln!("could not query due connections: {e}"),
        }

        match coinbase::due_for_sync(&pool, interval_seconds).await {
            Ok(due) => {
                for connection in due {
                    let result = async {
                        let client = open_coinbase(coinbase::credentials(&key, &connection)?).await?;
                        coinbase::sync(&pool, &connection, client, now_ms()?).await
                    }
                    .await;
                    match result {
                        Ok(()) => eprintln!("synced coinbase {}", connection.account_id),
                        Err(e) => eprintln!("coinbase sync failed for {}: {e}", connection.account_id),
                    }
                }
            }
            Err(e) => eprintln!("could not query due coinbase connections: {e}"),
        }

        match ibkr::due_for_sync(&pool, interval_seconds).await {
            Ok(due) => {
                for connection in due {
                    let result = async {
                        let client = open_ibkr(ibkr::credentials(&key, &connection)?).await?;
                        ibkr::sync(&pool, &connection, client, now_ms()?).await
                    }
                    .await;
                    match result {
                        Ok(()) => eprintln!("synced ibkr {}", connection.account_id),
                        Err(e) => eprintln!("ibkr sync failed for {}: {e}", connection.account_id),
                    }
                }
            }
            Err(e) => eprintln!("could not query due ibkr connections: {e}"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }
}

fn print_result<T: Serialize>(result: Result<T, String>) {
    match result {
        Ok(data) => {
            let mut value = serde_json::to_value(&data).unwrap_or(serde_json::json!({}));
            if let Some(obj) = value.as_object_mut() {
                obj.insert("ok".to_string(), serde_json::json!(true));
            }
            println!("{value}");
        }
        Err(error) => {
            println!("{}", serde_json::json!({"ok": false, "error": error}));
            std::process::exit(1);
        }
    }
}

#[tokio::main]
async fn main() {
    let command = std::env::args().nth(1).unwrap_or_default();
    match command.as_str() {
        "connect" => print_result(run_connect().await),
        "status" => print_result(run_status().await),
        "list" => print_result(run_list().await),
        "rename" => print_result(run_rename().await),
        "sync" => print_result(run_sync().await),
        "series" => print_result(run_series().await),
        "nav" => print_result(run_nav().await),
        "performance" => print_result(run_performance().await),
        #[cfg(feature = "proving")]
        "prove-performance" => print_result(run_prove_performance().await),
        #[cfg(not(feature = "proving"))]
        "prove-performance" => print_result::<()>(Err("this deployment was built without proving support".to_string())),
        "collector-identity" => print_result(run_collector_identity()),
        "scheduler" => {
            if let Err(e) = run_scheduler().await {
                eprintln!("scheduler exited: {e}");
                std::process::exit(1);
            }
        }
        other => {
            eprintln!(
                "unknown command '{other}'; expected connect|status|list|rename|sync|series|nav|performance|prove-performance|scheduler"
            );
            std::process::exit(2);
        }
    }
}
