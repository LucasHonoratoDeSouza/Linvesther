//! Postgres persistence for connections and synced data. Uses
//! `sqlx::query`/`query_as` (not the `query!`/`query_as!` macros) so
//! this crate compiles without a live database available at build
//! time — only at runtime, when the pool actually connects.

use binance_catalog::CatalogSnapshot;
use binance_flows::NormalizedFlow;
use binance_trades::Trade;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::crypto::EncryptedField;

#[derive(Debug, sqlx::FromRow)]
pub struct StoredConnection {
    pub id: Uuid,
    pub account_id: String,
    pub encrypted_api_key: Vec<u8>,
    pub nonce_api_key: Vec<u8>,
    pub encrypted_api_secret: Vec<u8>,
    pub nonce_api_secret: Vec<u8>,
    pub symbols: Vec<String>,
    pub status: String,
    pub last_synced_at: Option<DateTime<Utc>>,
}

pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_connection(
    pool: &PgPool,
    account_id: &str,
    api_key: &EncryptedField,
    api_secret: &EncryptedField,
    symbols: &[String],
) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO binance_connections
            (account_id, encrypted_api_key, nonce_api_key, encrypted_api_secret, nonce_api_secret, symbols)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (account_id) DO UPDATE SET
            encrypted_api_key = EXCLUDED.encrypted_api_key,
            nonce_api_key = EXCLUDED.nonce_api_key,
            encrypted_api_secret = EXCLUDED.encrypted_api_secret,
            nonce_api_secret = EXCLUDED.nonce_api_secret,
            symbols = EXCLUDED.symbols,
            status = 'active'
         RETURNING id",
    )
    .bind(account_id)
    .bind(&api_key.ciphertext)
    .bind(api_key.nonce.as_slice())
    .bind(&api_secret.ciphertext)
    .bind(api_secret.nonce.as_slice())
    .bind(symbols)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn find_connection_by_account(
    pool: &PgPool,
    account_id: &str,
) -> Result<Option<StoredConnection>, sqlx::Error> {
    sqlx::query_as::<_, StoredConnection>(
        "SELECT id, account_id, encrypted_api_key, nonce_api_key, encrypted_api_secret, nonce_api_secret, symbols, status, last_synced_at
         FROM binance_connections WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
}

/// Active connections never synced, or last synced more than
/// `interval_seconds` ago — what the scheduler loop polls for.
pub async fn connections_due_for_sync(
    pool: &PgPool,
    interval_seconds: i64,
) -> Result<Vec<StoredConnection>, sqlx::Error> {
    sqlx::query_as::<_, StoredConnection>(
        "SELECT id, account_id, encrypted_api_key, nonce_api_key, encrypted_api_secret, nonce_api_secret, symbols, status, last_synced_at
         FROM binance_connections
         WHERE status = 'active'
           AND (last_synced_at IS NULL OR last_synced_at < now() - make_interval(secs => $1))",
    )
    .bind(interval_seconds as f64)
    .fetch_all(pool)
    .await
}

pub async fn mark_synced(pool: &PgPool, connection_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE binance_connections SET last_synced_at = now() WHERE id = $1")
        .bind(connection_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn upsert_trades(
    pool: &PgPool,
    connection_id: Uuid,
    trades: &[Trade],
) -> Result<(), sqlx::Error> {
    for trade in trades {
        sqlx::query(
            "INSERT INTO binance_synced_trades
                (connection_id, symbol, trade_id, order_id, price, qty, commission, commission_asset, time_ms, is_buyer)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (connection_id, symbol, trade_id) DO NOTHING",
        )
        .bind(connection_id)
        .bind(&trade.symbol)
        .bind(trade.id as i64)
        .bind(trade.order_id as i64)
        .bind(&trade.price)
        .bind(&trade.qty)
        .bind(&trade.commission)
        .bind(&trade.commission_asset)
        .bind(trade.time_ms as i64)
        .bind(trade.is_buyer)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn upsert_flows(
    pool: &PgPool,
    connection_id: Uuid,
    flows: &[NormalizedFlow],
) -> Result<(), sqlx::Error> {
    for flow in flows {
        let legs_json = serde_json::to_value(
            flow.legs
                .iter()
                .map(|leg| serde_json::json!({"asset": leg.asset, "amount": leg.amount, "isCredit": leg.is_credit}))
                .collect::<Vec<_>>(),
        )
        .expect("legs always serialize");
        let fee_json = flow.fee.as_ref().map(|leg| {
            serde_json::json!({"asset": leg.asset, "amount": leg.amount, "isCredit": leg.is_credit})
        });
        sqlx::query(
            "INSERT INTO binance_synced_flows
                (connection_id, source_namespace, source_id, economic_time_ms, kind, legs_json, fee_json)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (connection_id, source_namespace, source_id) DO NOTHING",
        )
        .bind(connection_id)
        .bind(flow.source_namespace.to_string())
        .bind(&flow.source_id)
        .bind(flow.economic_time_ms as i64)
        .bind(&flow.kind)
        .bind(legs_json)
        .bind(fee_json)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// When the latest Simple Earn reward already stored was paid, if any — where the next
/// read of the rewards picks up.
pub async fn latest_earn_reward_ms(pool: &PgPool, connection_id: Uuid) -> Result<Option<u64>, sqlx::Error> {
    let (latest,): (Option<i64>,) = sqlx::query_as(
        "SELECT max(economic_time_ms) FROM binance_synced_flows WHERE connection_id = $1 AND source_id LIKE 'earn:%'",
    )
    .bind(connection_id)
    .fetch_one(pool)
    .await?;
    Ok(latest.map(|t| t as u64))
}

pub async fn upsert_catalog_snapshot(
    pool: &PgPool,
    connection_id: Uuid,
    snapshot: &CatalogSnapshot,
) -> Result<(), sqlx::Error> {
    for symbol in &snapshot.symbols {
        sqlx::query(
            "INSERT INTO binance_synced_catalog_symbols (connection_id, captured_at_ms, symbol)
             VALUES ($1, $2, $3)
             ON CONFLICT (connection_id, captured_at_ms, symbol) DO NOTHING",
        )
        .bind(connection_id)
        .bind(snapshot.captured_at_ms as i64)
        .bind(symbol)
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct ConnectionSummary {
    pub connected: bool,
    pub status: Option<String>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub trade_count: i64,
    pub flow_count: i64,
}

pub async fn connection_summary(
    pool: &PgPool,
    account_id: &str,
) -> Result<ConnectionSummary, sqlx::Error> {
    let Some(connection) = find_connection_by_account(pool, account_id).await? else {
        return Ok(ConnectionSummary {
            connected: false,
            status: None,
            last_synced_at: None,
            trade_count: 0,
            flow_count: 0,
        });
    };
    let (trade_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM binance_synced_trades WHERE connection_id = $1")
            .bind(connection.id)
            .fetch_one(pool)
            .await?;
    let (flow_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM binance_synced_flows WHERE connection_id = $1")
            .bind(connection.id)
            .fetch_one(pool)
            .await?;
    Ok(ConnectionSummary {
        connected: true,
        status: Some(connection.status),
        last_synced_at: connection.last_synced_at,
        trade_count,
        flow_count,
    })
}

#[derive(Debug, sqlx::FromRow)]
struct TradeRow {
    symbol: String,
    trade_id: i64,
    order_id: i64,
    price: String,
    qty: String,
    commission: String,
    commission_asset: String,
    time_ms: i64,
    is_buyer: bool,
}

/// Every trade already persisted for `connection_id`, chronological —
/// from both the historical `myTrades` backfill and the real-time
/// stream, indistinguishable once stored (both go through the same
/// `upsert_trades`/dedup path).
pub async fn fetch_stored_trades(
    pool: &PgPool,
    connection_id: Uuid,
) -> Result<Vec<Trade>, sqlx::Error> {
    let rows: Vec<TradeRow> = sqlx::query_as(
        "SELECT symbol, trade_id, order_id, price, qty, commission, commission_asset, time_ms, is_buyer
         FROM binance_synced_trades WHERE connection_id = $1 ORDER BY time_ms ASC",
    )
    .bind(connection_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Trade {
            symbol: r.symbol,
            id: r.trade_id as u64,
            order_id: r.order_id as u64,
            price: r.price,
            qty: r.qty,
            commission: r.commission,
            commission_asset: r.commission_asset,
            time_ms: r.time_ms as u64,
            is_buyer: r.is_buyer,
        })
        .collect())
}

#[derive(Debug, sqlx::FromRow)]
struct FlowRow {
    source_namespace: String,
    source_id: String,
    economic_time_ms: i64,
    kind: String,
    legs_json: serde_json::Value,
    fee_json: Option<serde_json::Value>,
}

fn parse_flow_family(s: &str) -> Option<binance_flows::FlowFamily> {
    use binance_flows::FlowFamily::*;
    match s {
        "deposit" => Some(Deposit),
        "withdrawal" => Some(Withdrawal),
        "transfer" => Some(Transfer),
        "convert" => Some(Convert),
        "dust" => Some(Dust),
        "dividend" => Some(Dividend),
        _ => None,
    }
}

fn parse_asset_leg(value: &serde_json::Value) -> Option<binance_flows::AssetLeg> {
    Some(binance_flows::AssetLeg {
        asset: value.get("asset")?.as_str()?.to_string(),
        amount: value.get("amount")?.as_str()?.to_string(),
        is_credit: value.get("isCredit")?.as_bool()?,
    })
}

/// Every flow already persisted for `connection_id`, chronological.
/// Rows with a `source_namespace` this version of the worker doesn't
/// recognize are skipped (fail-closed on the ledger conversion that
/// consumes this, not silently miscategorized).
pub async fn fetch_stored_flows(
    pool: &PgPool,
    connection_id: Uuid,
) -> Result<Vec<NormalizedFlow>, sqlx::Error> {
    let rows: Vec<FlowRow> = sqlx::query_as(
        "SELECT source_namespace, source_id, economic_time_ms, kind, legs_json, fee_json
         FROM binance_synced_flows WHERE connection_id = $1 ORDER BY economic_time_ms ASC",
    )
    .bind(connection_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            Some(NormalizedFlow {
                source_namespace: parse_flow_family(&r.source_namespace)?,
                source_id: r.source_id,
                economic_time_ms: r.economic_time_ms as u64,
                kind: r.kind,
                legs: r
                    .legs_json
                    .as_array()?
                    .iter()
                    .filter_map(parse_asset_leg)
                    .collect(),
                fee: r.fee_json.as_ref().and_then(parse_asset_leg),
            })
        })
        .collect())
}

/// When this connection was first made, in epoch milliseconds — the
/// instant every performance figure is measured from. Reconnecting the
/// same account with new credentials keeps this (see
/// `insert_connection`'s upsert), so a track record can't be restarted
/// just by reconnecting.
pub async fn connection_created_at_ms(pool: &PgPool, connection_id: Uuid) -> Result<u64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT FROM binance_connections WHERE id = $1",
    )
    .bind(connection_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0 as u64)
}

/// A person's own name for a connection ("Main", "Long-term", ...) —
/// only how they tell several connected accounts apart.
/// Deletes the stored credential and, by cascade, everything collected for it
/// (trades, flows, catalog). Returns whether a connection was removed.
pub async fn delete_connection(pool: &PgPool, connection_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM binance_connections WHERE id = $1")
        .bind(connection_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_connection_label(pool: &PgPool, connection_id: Uuid, label: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE binance_connections SET label = $2 WHERE id = $1")
        .bind(connection_id)
        .bind(label)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct OwnedConnection {
    #[serde(rename = "accountId")]
    pub account_id: String,
    pub label: Option<String>,
    #[serde(rename = "connectedAtMs")]
    pub connected_at_ms: i64,
    pub status: String,
    pub broker: String,
}

/// Every connection belonging to `owner_address`: the account whose id
/// is the address itself (the first one) plus any `<address>_<name>`
/// added later. `_` is escaped so it matches literally, not as a LIKE
/// wildcard.
pub async fn connections_owned_by(pool: &PgPool, owner_address: &str) -> Result<Vec<OwnedConnection>, sqlx::Error> {
    sqlx::query_as(
        "SELECT account_id, label, (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS connected_at_ms, status, 'binance' AS broker
         FROM binance_connections
         WHERE lower(account_id) = lower($1) OR lower(account_id) LIKE lower($1) || '\\_%' ESCAPE '\\'
         ORDER BY created_at",
    )
    .bind(owner_address)
    .fetch_all(pool)
    .await
}
