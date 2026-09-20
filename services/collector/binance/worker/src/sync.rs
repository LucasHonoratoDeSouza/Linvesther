//! One sync cycle: fetch real data via [`binance_live_client::LiveClient`]
//! and persist it via [`crate::db`]. The HTTP client is blocking (see
//! its own doc comment), so every call into it from an async context
//! goes through [`tokio::task::spawn_blocking`].

use binance_catalog::CatalogSnapshot;
use binance_flows::NormalizedFlow;
use binance_live_client::{LiveClient, LiveClientError};
use binance_trades::Trade;
use collector_credentials::Environment;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Client(#[from] LiveClientError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error("background task panicked: {0}")]
    Join(#[from] tokio::task::JoinError),
}

#[derive(Debug, serde::Serialize)]
pub struct SyncSummary {
    pub trades_fetched: usize,
    pub flows_fetched: usize,
    pub catalog_symbols_fetched: usize,
}

struct RawFetch {
    trades: Vec<Trade>,
    flows: Vec<NormalizedFlow>,
    catalog: Option<CatalogSnapshot>,
}

/// The blocking part: confirms the key is still read-only (permissions
/// can change after a connection was first created — this is checked
/// every cycle, not only at connect time), then fetches trades for
/// every tracked symbol plus the account's full deposit/withdrawal
/// history and the current exchange catalog.
fn fetch_blocking(
    api_key: String,
    api_secret: String,
    symbols: Vec<String>,
) -> Result<RawFetch, LiveClientError> {
    let mut client = LiveClient::new(api_key, api_secret, Environment::Production);
    client.ensure_read_only()?;

    let mut trades = Vec::new();
    for symbol in &symbols {
        trades.extend(client.fetch_my_trades(symbol)?);
    }

    let mut flows = client.fetch_deposits()?;
    flows.extend(client.fetch_withdrawals()?);

    let catalog = client.fetch_exchange_info().ok();

    Ok(RawFetch {
        trades,
        flows,
        catalog,
    })
}

/// Runs one full sync cycle for `connection_id` and persists the
/// result, updating `last_synced_at` only on success — a failed cycle
/// leaves the connection due for retry on the next scheduler tick
/// rather than silently marking it as up to date.
pub async fn sync_and_persist(
    pool: &PgPool,
    connection_id: Uuid,
    api_key: String,
    api_secret: String,
    symbols: Vec<String>,
) -> Result<SyncSummary, SyncError> {
    let fetch =
        tokio::task::spawn_blocking(move || fetch_blocking(api_key, api_secret, symbols)).await??;

    db::upsert_trades(pool, connection_id, &fetch.trades).await?;
    db::upsert_flows(pool, connection_id, &fetch.flows).await?;
    if let Some(catalog) = &fetch.catalog {
        db::upsert_catalog_snapshot(pool, connection_id, catalog).await?;
    }
    db::mark_synced(pool, connection_id).await?;

    Ok(SyncSummary {
        trades_fetched: fetch.trades.len(),
        flows_fetched: fetch.flows.len(),
        catalog_symbols_fetched: fetch.catalog.map(|c| c.symbols.len()).unwrap_or(0),
    })
}
