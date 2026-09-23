//! Kraken connections: storage, syncing, and turning what Kraken reports into
//! the same trades and flows the other exchanges feed the performance engine.
//!
//! Unlike Coinbase, Kraken keeps a *ledger*: one line for every change to a
//! balance, whatever caused it. So deposits and withdrawals are read from it
//! directly rather than inferred from balance snapshots. Trades come from the
//! trade history (their ledger lines are skipped so nothing counts twice), and
//! every other ledger line still moves a balance, so it is kept as a flow that
//! changes holdings without being counted as money put in or taken out.

use crate::coinbase::stable_id;
use crate::crypto::{credential_context, decrypt, EncryptedField, MasterKey};
use binance_flows::{AssetLeg, FlowFamily, NormalizedFlow};
use binance_trades::Trade;
use chrono::{DateTime, Utc};
use kraken_client::{Credentials, KrakenClient, LedgerEntry, Trade as KrakenTrade};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::time::Duration;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct KrakenConnection {
    pub id: Uuid,
    pub account_id: String,
    pub encrypted_api_key: Vec<u8>,
    pub nonce_api_key: Vec<u8>,
    pub encrypted_api_secret: Vec<u8>,
    pub nonce_api_secret: Vec<u8>,
    pub label: Option<String>,
    pub status: String,
    pub last_synced_at: Option<DateTime<Utc>>,
}

pub fn credentials(key: &MasterKey, connection: &KrakenConnection) -> Result<Credentials, String> {
    let nonce_key: [u8; 12] = connection.nonce_api_key.clone().try_into().map_err(|_| "corrupt stored API key")?;
    let nonce_secret: [u8; 12] = connection.nonce_api_secret.clone().try_into().map_err(|_| "corrupt stored API secret")?;
    let api_key = decrypt(key, &connection.encrypted_api_key, &nonce_key, &credential_context("kraken", &connection.account_id, "api_key")).map_err(|e| e.to_string())?;
    let api_secret = decrypt(key, &connection.encrypted_api_secret, &nonce_secret, &credential_context("kraken", &connection.account_id, "api_secret")).map_err(|e| e.to_string())?;
    Credentials::new(&api_key, &api_secret).map_err(|e| e.to_string())
}

const COLUMNS: &str = "id, account_id, encrypted_api_key, nonce_api_key, encrypted_api_secret, nonce_api_secret, label, status, last_synced_at";

// ---------- storage ----------------------------------------------------

/// Adds the connection, or replaces the credentials of an existing one —
/// keeping its `created_at`, so a track record can't be restarted by
/// reconnecting.
pub async fn insert_connection(pool: &PgPool, account_id: &str, api_key: &EncryptedField, api_secret: &EncryptedField) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO kraken_connections
            (account_id, encrypted_api_key, nonce_api_key, encrypted_api_secret, nonce_api_secret)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (account_id) DO UPDATE SET
            encrypted_api_key = EXCLUDED.encrypted_api_key,
            nonce_api_key = EXCLUDED.nonce_api_key,
            encrypted_api_secret = EXCLUDED.encrypted_api_secret,
            nonce_api_secret = EXCLUDED.nonce_api_secret,
            status = 'active'
         RETURNING id",
    )
    .bind(account_id)
    .bind(&api_key.ciphertext)
    .bind(api_key.nonce.as_slice())
    .bind(&api_secret.ciphertext)
    .bind(api_secret.nonce.as_slice())
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Deletes the stored credential and, by cascade, everything collected for it.
pub async fn delete_connection(pool: &PgPool, connection_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM kraken_connections WHERE id = $1").bind(connection_id).execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_label(pool: &PgPool, connection_id: Uuid, label: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE kraken_connections SET label = $2 WHERE id = $1").bind(connection_id).bind(label).execute(pool).await?;
    Ok(())
}

pub async fn find_by_account(pool: &PgPool, account_id: &str) -> Result<Option<KrakenConnection>, sqlx::Error> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM kraken_connections WHERE account_id = $1")).bind(account_id).fetch_optional(pool).await
}

pub async fn owned_by(pool: &PgPool, owner_address: &str) -> Result<Vec<crate::db::OwnedConnection>, sqlx::Error> {
    sqlx::query_as(
        "SELECT account_id, label, (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS connected_at_ms, status, 'kraken' AS broker
         FROM kraken_connections
         WHERE lower(account_id) = lower($1) OR lower(account_id) LIKE lower($1) || '\\_%' ESCAPE '\\'
         ORDER BY created_at",
    )
    .bind(owner_address)
    .fetch_all(pool)
    .await
}

pub async fn due_for_sync(pool: &PgPool, interval_seconds: i64) -> Result<Vec<KrakenConnection>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM kraken_connections
         WHERE status = 'active'
           AND (last_synced_at IS NULL OR last_synced_at < now() - make_interval(secs => $1))"
    ))
    .bind(interval_seconds as f64)
    .fetch_all(pool)
    .await
}

pub async fn created_at_ms(pool: &PgPool, connection_id: Uuid) -> Result<u64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT FROM kraken_connections WHERE id = $1")
        .bind(connection_id)
        .fetch_one(pool)
        .await?;
    Ok(row.0 as u64)
}

pub async fn insert_trades(pool: &PgPool, connection_id: Uuid, trades: &[KrakenTrade]) -> Result<(), sqlx::Error> {
    for trade in trades {
        sqlx::query(
            "INSERT INTO kraken_trades
                (connection_id, trade_id, order_id, symbol, base, quote, is_buy, price, volume, fee, time_ms)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT (connection_id, trade_id) DO NOTHING",
        )
        .bind(connection_id)
        .bind(&trade.id)
        .bind(&trade.order_id)
        .bind(&trade.symbol)
        .bind(&trade.base)
        .bind(&trade.quote)
        .bind(trade.is_buy)
        .bind(&trade.price)
        .bind(&trade.volume)
        .bind(&trade.fee)
        .bind(trade.time_ms as i64)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn insert_ledger(pool: &PgPool, connection_id: Uuid, entries: &[LedgerEntry]) -> Result<(), sqlx::Error> {
    for entry in entries {
        sqlx::query(
            "INSERT INTO kraken_ledger
                (connection_id, ledger_id, refid, time_ms, entry_type, subtype, asset, amount, fee)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (connection_id, ledger_id) DO NOTHING",
        )
        .bind(connection_id)
        .bind(&entry.id)
        .bind(&entry.refid)
        .bind(entry.time_ms as i64)
        .bind(&entry.kind)
        .bind(&entry.subtype)
        .bind(&entry.asset)
        .bind(&entry.amount)
        .bind(&entry.fee)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn latest_time(pool: &PgPool, table: &str, connection_id: Uuid) -> Result<Option<u64>, sqlx::Error> {
    let row: (Option<i64>,) = sqlx::query_as(&format!("SELECT max(time_ms) FROM {table} WHERE connection_id = $1")).bind(connection_id).fetch_one(pool).await?;
    Ok(row.0.map(|ms| ms as u64))
}

pub async fn stored_trade_count(pool: &PgPool, connection_id: Uuid) -> Result<i64, sqlx::Error> {
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM kraken_trades WHERE connection_id = $1").bind(connection_id).fetch_one(pool).await?;
    Ok(count)
}

// ---------- syncing -----------------------------------------------------

/// Kraken limits how often a key may call its private API (a small budget that
/// refills slowly), and a page of results costs more than most calls. So a
/// connection is synced by one caller at a time, and not again within this long
/// unless asked outright — several requests arriving together share one sync.
const MIN_SYNC_GAP: Duration = Duration::from_secs(20);
/// How long a confirmation that the key cannot trade or withdraw is trusted.
const REVERIFY_AFTER: Duration = Duration::from_secs(600);

/// A key derived from the connection's id, for the advisory lock that keeps two
/// syncs of one connection from running together.
fn lock_key(connection_id: Uuid) -> i64 {
    i64::from_le_bytes(connection_id.as_bytes()[..8].try_into().expect("a UUID has 16 bytes"))
}

/// The lock that lets one request at a time check the key's permissions.
fn verify_lock_key(connection_id: Uuid) -> i64 {
    lock_key(connection_id) ^ 0x5eed_c0de
}

/// Records that the key was just confirmed unable to place orders or withdraw.
pub async fn mark_verified(pool: &PgPool, connection_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE kraken_connections SET verified_read_only_at = now() WHERE id = $1").bind(connection_id).execute(pool).await?;
    Ok(())
}

/// Confirms again that the key cannot place orders or withdraw, if it has been a
/// while: a key's permissions can be widened at Kraken after it was connected. Several
/// requests often arrive together; one checks and the others rely on it, so the key is not
/// tried a dozen times at once.
pub async fn verify_if_due(pool: &PgPool, connection_id: Uuid, client: std::sync::Arc<KrakenClient>) -> Result<(), String> {
    let (recent,): (bool,) = sqlx::query_as("SELECT coalesce(verified_read_only_at > now() - make_interval(secs => $2), false) FROM kraken_connections WHERE id = $1")
        .bind(connection_id)
        .bind(REVERIFY_AFTER.as_secs() as f64)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
    if recent {
        return Ok(());
    }
    let mut guard = pool.acquire().await.map_err(|e| e.to_string())?;
    let (acquired,): (bool,) = sqlx::query_as("SELECT pg_try_advisory_lock($1)").bind(verify_lock_key(connection_id)).fetch_one(&mut *guard).await.map_err(|e| e.to_string())?;
    if !acquired {
        return Ok(());
    }
    let result = tokio::task::spawn_blocking(move || client.ensure_cannot_write().map_err(|e| e.to_string())).await.map_err(|e| e.to_string()).and_then(|r| r);
    let recorded = if result.is_ok() { mark_verified(pool, connection_id).await.map_err(|e| e.to_string()) } else { Ok(()) };
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)").bind(verify_lock_key(connection_id)).execute(&mut *guard).await;
    result.and(recorded)
}

/// Brings the stored trades and ledger up to date. Each is asked for only what
/// is newer than what is already held (a repeat at the boundary is ignored),
/// never reaching back before the connection. `force` syncs even if the
/// connection was synced moments ago; another sync already running counts as done.
/// Blocking HTTP, so the calls run under `spawn_blocking`.
pub async fn sync(pool: &PgPool, connection: &KrakenConnection, client: std::sync::Arc<KrakenClient>, force: bool) -> Result<(), String> {
    // The lock belongs to one database connection, so that connection is held until it is released.
    let mut guard = pool.acquire().await.map_err(|e| e.to_string())?;
    let (acquired,): (bool,) = sqlx::query_as("SELECT pg_try_advisory_lock($1)").bind(lock_key(connection.id)).fetch_one(&mut *guard).await.map_err(|e| e.to_string())?;
    if !acquired {
        return Ok(());
    }
    let result = sync_locked(pool, connection, client, force).await;
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)").bind(lock_key(connection.id)).execute(&mut *guard).await;
    result
}

async fn sync_locked(pool: &PgPool, connection: &KrakenConnection, client: std::sync::Arc<KrakenClient>, force: bool) -> Result<(), String> {
    if !force {
        let (recent,): (bool,) = sqlx::query_as("SELECT coalesce(last_synced_at > now() - make_interval(secs => $2), false) FROM kraken_connections WHERE id = $1")
            .bind(connection.id)
            .bind(MIN_SYNC_GAP.as_secs() as f64)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;
        if recent {
            return Ok(());
        }
    }
    let created_at = created_at_ms(pool, connection.id).await.map_err(|e| e.to_string())?;
    let trades_from = latest_time(pool, "kraken_trades", connection.id).await.map_err(|e| e.to_string())?.unwrap_or(created_at).max(created_at);
    let ledger_from = latest_time(pool, "kraken_ledger", connection.id).await.map_err(|e| e.to_string())?.unwrap_or(created_at).max(created_at);

    let fetch = client.clone();
    let (trades, ledger) = tokio::task::spawn_blocking(move || {
        let trades = fetch.trades_since(trades_from).map_err(|e| e.to_string())?;
        let ledger = fetch.ledger_since(ledger_from).map_err(|e| e.to_string())?;
        Ok::<_, String>((trades, ledger))
    })
    .await
    .map_err(|e| e.to_string())??;

    insert_trades(pool, connection.id, &trades).await.map_err(|e| e.to_string())?;
    insert_ledger(pool, connection.id, &ledger).await.map_err(|e| e.to_string())?;
    sqlx::query("UPDATE kraken_connections SET last_synced_at = now() WHERE id = $1").bind(connection.id).execute(pool).await.map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- what the engine reads ---------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LedgerRow {
    pub ledger_id: String,
    pub refid: String,
    pub time_ms: i64,
    pub entry_type: String,
    pub subtype: String,
    pub asset: String,
    pub amount: String,
    pub fee: String,
}

#[derive(sqlx::FromRow)]
struct TradeRow {
    trade_id: String,
    order_id: String,
    symbol: String,
    quote: String,
    is_buy: bool,
    price: String,
    volume: String,
    fee: String,
    time_ms: i64,
}

/// Stored trades, as the trades the engine understands. The fee is charged in
/// the quote currency.
pub async fn trades(pool: &PgPool, connection_id: Uuid) -> Result<Vec<Trade>, sqlx::Error> {
    let rows: Vec<TradeRow> = sqlx::query_as(
        "SELECT trade_id, order_id, symbol, quote, is_buy, price, volume, fee, time_ms
         FROM kraken_trades WHERE connection_id = $1 ORDER BY time_ms, trade_id",
    )
    .bind(connection_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| Trade {
            symbol: row.symbol,
            id: stable_id(&row.trade_id),
            order_id: stable_id(&row.order_id),
            price: row.price,
            qty: row.volume,
            commission: row.fee,
            commission_asset: row.quote,
            time_ms: row.time_ms as u64,
            is_buyer: row.is_buy,
        })
        .collect())
}

/// Stored ledger lines, classified as the flows the engine understands.
pub async fn flows(pool: &PgPool, connection_id: Uuid) -> Result<Vec<NormalizedFlow>, String> {
    let rows: Vec<LedgerRow> = sqlx::query_as(
        "SELECT ledger_id, refid, time_ms, entry_type, subtype, asset, amount, fee
         FROM kraken_ledger WHERE connection_id = $1 ORDER BY time_ms, ledger_id",
    )
    .bind(connection_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    classify(&rows)
}

fn decimal(value: &str, what: &str) -> Result<Decimal, String> {
    Decimal::from_str(value).map_err(|_| format!("a Kraken ledger line had an unreadable {what}: {value:?}"))
}

/// What the line did to the balance: its amount less the fee.
fn net(row: &LedgerRow) -> Result<Decimal, String> {
    Ok(decimal(&row.amount, "amount")? - decimal(&row.fee, "fee")?)
}

fn signed_leg(asset: &str, change: Decimal) -> AssetLeg {
    if change.is_sign_negative() {
        AssetLeg::debit(asset, change.abs().to_string())
    } else {
        AssetLeg::credit(asset, change.to_string())
    }
}

/// True for the lines that only move money between a person's own Kraken wallets
/// (spot to staking and back): with staked assets counted as the asset itself,
/// they net to nothing, so they are left out.
fn is_wallet_move(row: &LedgerRow) -> bool {
    let subtype = row.subtype.to_lowercase();
    match row.entry_type.as_str() {
        "transfer" => subtype.contains("staking") || subtype.contains("earn"),
        "earn" => subtype != "reward",
        _ => false,
    }
}

/// Turns ledger lines into flows. Only deposits and withdrawals are money put in
/// or taken out; the rest change what the account holds without being counted as
/// capital (a staking reward is performance, not a deposit). Every line that is
/// kept moves a balance, so what the account held at any past moment can be rebuilt
/// from the current balance; nothing is dropped except what would count twice
/// (trades, which come from the trade history) or cancel out (wallet moves).
pub fn classify(rows: &[LedgerRow]) -> Result<Vec<NormalizedFlow>, String> {
    let mut flows = Vec::new();
    // Conversions have a line per asset that belong together.
    let mut conversions: BTreeMap<String, Vec<&LedgerRow>> = BTreeMap::new();

    for row in rows {
        if row.entry_type == "trade" || is_wallet_move(row) {
            continue;
        }
        let time = row.time_ms as u64;
        let source_id = format!("kraken:{}", row.ledger_id);
        let fee = decimal(&row.fee, "fee")?;
        let fee_leg = (!fee.is_zero()).then(|| AssetLeg::debit(&row.asset, fee.to_string()));
        let subtype = row.subtype.to_lowercase();
        match row.entry_type.as_str() {
            "deposit" | "withdrawal" => {
                let amount = decimal(&row.amount, "amount")?;
                let (family, leg) = if row.entry_type == "deposit" {
                    (FlowFamily::Deposit, AssetLeg::credit(&row.asset, amount.abs().to_string()))
                } else {
                    (FlowFamily::Withdrawal, AssetLeg::debit(&row.asset, amount.abs().to_string()))
                };
                flows.push(NormalizedFlow { source_namespace: family, source_id, economic_time_ms: time, kind: row.entry_type.clone(), legs: vec![leg], fee: fee_leg });
            }
            "spend" | "receive" | "adjustment" => conversions.entry(if row.refid.is_empty() { row.ledger_id.clone() } else { row.refid.clone() }).or_default().push(row),
            // Wallets outside spot: leaving for Kraken Futures is money leaving this account.
            "transfer" if subtype.contains("futures") => {
                let change = net(row)?;
                if change.is_zero() {
                    continue;
                }
                let family = if change.is_sign_negative() { FlowFamily::Withdrawal } else { FlowFamily::Deposit };
                flows.push(NormalizedFlow { source_namespace: family, source_id, economic_time_ms: time, kind: format!("transfer {}", row.subtype), legs: vec![signed_leg(&row.asset, change)], fee: None });
            }
            other => {
                let change = net(row)?;
                if change.is_zero() {
                    continue;
                }
                let family = match other {
                    "staking" | "dividend" | "credit" | "earn" => FlowFamily::Dividend,
                    _ => FlowFamily::Transfer,
                };
                flows.push(NormalizedFlow { source_namespace: family, source_id, economic_time_ms: time, kind: other.to_string(), legs: vec![signed_leg(&row.asset, change)], fee: None });
            }
        }
    }

    for (group, lines) in conversions {
        let mut legs = Vec::new();
        for line in &lines {
            let change = net(line)?;
            if !change.is_zero() {
                legs.push(signed_leg(&line.asset, change));
            }
        }
        if legs.is_empty() {
            continue;
        }
        let time = lines.iter().map(|l| l.time_ms).min().unwrap_or_default() as u64;
        flows.push(NormalizedFlow { source_namespace: FlowFamily::Convert, source_id: format!("kraken:{group}"), economic_time_ms: time, kind: "convert".to_string(), legs, fee: None });
    }
    flows.sort_by(|a, b| (a.economic_time_ms, &a.source_id).cmp(&(b.economic_time_ms, &b.source_id)));
    Ok(flows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(id, refid, kind, subtype)` and `(asset, amount, fee)` of a line, at `time_ms`.
    fn row(line: (&str, &str, &str, &str), money: (&str, &str, &str), time_ms: i64) -> LedgerRow {
        LedgerRow { ledger_id: line.0.into(), refid: line.1.into(), time_ms, entry_type: line.2.into(), subtype: line.3.into(), asset: money.0.into(), amount: money.1.into(), fee: money.2.into() }
    }
    fn one(rows: &[LedgerRow]) -> NormalizedFlow {
        let flows = classify(rows).unwrap();
        assert_eq!(flows.len(), 1, "{flows:?}");
        flows.into_iter().next().unwrap()
    }

    #[test]
    fn a_deposit_is_money_in_with_its_fee_kept_apart() {
        let flow = one(&[row(("L1", "R1", "deposit", ""), ("USD", "100.0000", "0.5000"), 1000)]);
        assert_eq!(flow.source_namespace, FlowFamily::Deposit);
        assert_eq!(flow.legs, vec![AssetLeg::credit("USD", "100.0000")]);
        assert_eq!(flow.fee, Some(AssetLeg::debit("USD", "0.5000")));
        assert_eq!(flow.economic_time_ms, 1000);
    }

    #[test]
    fn a_withdrawal_is_money_out_whose_amount_kraken_reports_negative() {
        let flow = one(&[row(("L2", "R2", "withdrawal", ""), ("BTC", "-0.5000000000", "0.0005000000"), 2000)]);
        assert_eq!(flow.source_namespace, FlowFamily::Withdrawal);
        assert_eq!(flow.legs, vec![AssetLeg::debit("BTC", "0.5000000000")]);
        assert_eq!(flow.fee, Some(AssetLeg::debit("BTC", "0.0005000000")));
    }

    #[test]
    fn trades_are_left_to_the_trade_history_so_nothing_counts_twice() {
        assert!(classify(&[row(("L3", "R3", "trade", ""), ("BTC", "0.01", "0"), 3000), row(("L4", "R3", "trade", ""), ("USD", "-500", "1.2"), 3000)]).unwrap().is_empty());
    }

    #[test]
    fn moves_between_spot_and_staking_cancel_out_and_are_left_out() {
        let rows = [
            row(("L5", "R5", "transfer", "spottostaking"), ("DOT", "-10", "0"), 4000),
            row(("L6", "R5", "transfer", "stakingfromspot"), ("DOT", "10", "0"), 4000),
            row(("L7", "R6", "earn", "allocation"), ("DOT", "-10", "0"), 4100),
            row(("L8", "R6", "earn", "allocation"), ("DOT", "10", "0"), 4100),
        ];
        assert!(classify(&rows).unwrap().is_empty());
    }

    #[test]
    fn a_reward_grows_holdings_but_is_performance_not_a_deposit() {
        let staking = one(&[row(("L9", "R9", "staking", ""), ("DOT", "0.02", "0"), 5000)]);
        assert_eq!(staking.source_namespace, FlowFamily::Dividend);
        assert_eq!(staking.legs, vec![AssetLeg::credit("DOT", "0.02")]);
        let earn = one(&[row(("L10", "R10", "earn", "reward"), ("USD", "0.31", "0"), 5100)]);
        assert_eq!(earn.source_namespace, FlowFamily::Dividend);
    }

    #[test]
    fn a_conversion_is_one_flow_with_a_leg_per_asset_and_its_fee_folded_in() {
        let rows = [row(("L11", "RC", "spend", ""), ("USD", "-100.0000", "1.0000"), 6000), row(("L12", "RC", "receive", ""), ("BTC", "0.00100000", "0"), 6000)];
        let flow = one(&rows);
        assert_eq!(flow.source_namespace, FlowFamily::Convert);
        assert_eq!(flow.legs, vec![AssetLeg::debit("USD", "101.0000"), AssetLeg::credit("BTC", "0.00100000")]);
        assert_eq!(flow.source_id, "kraken:RC");
    }

    #[test]
    fn leaving_for_futures_is_money_out_and_coming_back_is_money_in() {
        let out = one(&[row(("L13", "R13", "transfer", "spottofutures"), ("USD", "-50", "0"), 7000)]);
        assert_eq!(out.source_namespace, FlowFamily::Withdrawal);
        let back = one(&[row(("L14", "R14", "transfer", "spotfromfutures"), ("USD", "20", "0"), 7100)]);
        assert_eq!(back.source_namespace, FlowFamily::Deposit);
    }

    #[test]
    fn a_line_of_a_kind_not_recognised_still_moves_the_balance_without_being_counted_as_capital() {
        let flow = one(&[row(("L15", "R15", "rollover", ""), ("USD", "-0.12", "0"), 8000)]);
        assert_eq!(flow.source_namespace, FlowFamily::Transfer);
        assert_eq!(flow.legs, vec![AssetLeg::debit("USD", "0.12")]);
    }

    #[test]
    fn a_line_that_changes_nothing_is_dropped_and_an_unreadable_amount_fails_closed() {
        assert!(classify(&[row(("L16", "R16", "adjustment", ""), ("USD", "0", "0"), 9000)]).unwrap().is_empty());
        assert!(classify(&[row(("L17", "R17", "deposit", ""), ("USD", "lots", "0"), 9000)]).is_err());
    }

    #[test]
    fn flows_come_out_oldest_first() {
        let rows = [row(("B", "R", "deposit", ""), ("USD", "1", "0"), 2000), row(("A", "R", "deposit", ""), ("USD", "1", "0"), 1000)];
        let flows = classify(&rows).unwrap();
        assert_eq!(flows.iter().map(|f| f.economic_time_ms).collect::<Vec<_>>(), vec![1000, 2000]);
    }
}
