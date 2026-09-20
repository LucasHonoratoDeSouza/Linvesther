//! Coinbase connections: storage, syncing, and turning what Coinbase
//! reports into the same trades and flows the Binance path feeds the
//! performance engine.
//!
//! Coinbase gives reliable *fills* (executed trades) and *balances*, but no
//! stable list of deposits and withdrawals. So those are found by
//! reconciliation: between two balance snapshots, whatever change the fills
//! don't explain must have come from outside — a deposit if the balance
//! rose, a withdrawal if it fell.

use crate::crypto::{credential_context, decrypt, EncryptedField, MasterKey};
use binance_flows::{AssetLeg, FlowFamily, NormalizedFlow};
use binance_trades::Trade;
use chrono::{DateTime, Utc};
use coinbase_client::wire::Fill;
use coinbase_client::{CoinbaseClient, Credentials};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use uuid::Uuid;

/// A fill this recent may not be in the balances Coinbase reports yet (or
/// the reverse), so reconciling right then would invent a transfer.
const SETTLE_MS: u64 = 5_000;
/// Rounding noise is not a transfer: below this an unexplained difference
/// is ignored — a hundredth of a cent-worth for dollars, a satoshi-scale
/// amount for everything else.
const DUST_QUOTE: &str = "0.01";
const DUST_OTHER: &str = "0.00000001";

#[derive(Debug, sqlx::FromRow)]
pub struct CoinbaseConnection {
    pub id: Uuid,
    pub account_id: String,
    pub encrypted_key_name: Vec<u8>,
    pub nonce_key_name: Vec<u8>,
    pub encrypted_private_key: Vec<u8>,
    pub nonce_private_key: Vec<u8>,
    pub label: Option<String>,
    pub status: String,
    pub last_synced_at: Option<DateTime<Utc>>,
}

pub fn credentials(key: &MasterKey, connection: &CoinbaseConnection) -> Result<Credentials, String> {
    let nonce_name: [u8; 12] = connection.nonce_key_name.clone().try_into().map_err(|_| "corrupt stored key name")?;
    let nonce_key: [u8; 12] = connection.nonce_private_key.clone().try_into().map_err(|_| "corrupt stored private key")?;
    let key_name = decrypt(key, &connection.encrypted_key_name, &nonce_name, &credential_context("coinbase", &connection.account_id, "key_name")).map_err(|e| e.to_string())?;
    let private_key = decrypt(key, &connection.encrypted_private_key, &nonce_key, &credential_context("coinbase", &connection.account_id, "private_key")).map_err(|e| e.to_string())?;
    Credentials::new(&key_name, &private_key).map_err(|e| e.to_string())
}

// ---------- storage ----------------------------------------------------

/// Adds the connection, or replaces the credentials of an existing one —
/// keeping its `created_at`, so a track record can't be restarted by
/// reconnecting.
pub async fn insert_connection(
    pool: &PgPool,
    account_id: &str,
    key_name: &EncryptedField,
    private_key: &EncryptedField,
) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO coinbase_connections
            (account_id, encrypted_key_name, nonce_key_name, encrypted_private_key, nonce_private_key)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (account_id) DO UPDATE SET
            encrypted_key_name = EXCLUDED.encrypted_key_name,
            nonce_key_name = EXCLUDED.nonce_key_name,
            encrypted_private_key = EXCLUDED.encrypted_private_key,
            nonce_private_key = EXCLUDED.nonce_private_key,
            status = 'active'
         RETURNING id",
    )
    .bind(account_id)
    .bind(&key_name.ciphertext)
    .bind(key_name.nonce.as_slice())
    .bind(&private_key.ciphertext)
    .bind(private_key.nonce.as_slice())
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Deletes the stored credential and, by cascade, everything collected for it.
pub async fn delete_connection(pool: &PgPool, connection_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM coinbase_connections WHERE id = $1")
        .bind(connection_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_label(pool: &PgPool, connection_id: Uuid, label: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE coinbase_connections SET label = $2 WHERE id = $1")
        .bind(connection_id)
        .bind(label)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn find_by_account(pool: &PgPool, account_id: &str) -> Result<Option<CoinbaseConnection>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, account_id, encrypted_key_name, nonce_key_name, encrypted_private_key, nonce_private_key, label, status, last_synced_at
         FROM coinbase_connections WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
}

pub async fn owned_by(pool: &PgPool, owner_address: &str) -> Result<Vec<crate::db::OwnedConnection>, sqlx::Error> {
    sqlx::query_as(
        "SELECT account_id, label, (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS connected_at_ms, status, 'coinbase' AS broker
         FROM coinbase_connections
         WHERE lower(account_id) = lower($1) OR lower(account_id) LIKE lower($1) || '\\_%' ESCAPE '\\'
         ORDER BY created_at",
    )
    .bind(owner_address)
    .fetch_all(pool)
    .await
}

pub async fn due_for_sync(pool: &PgPool, interval_seconds: i64) -> Result<Vec<CoinbaseConnection>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, account_id, encrypted_key_name, nonce_key_name, encrypted_private_key, nonce_private_key, label, status, last_synced_at
         FROM coinbase_connections
         WHERE status = 'active'
           AND (last_synced_at IS NULL OR last_synced_at < now() - make_interval(secs => $1))",
    )
    .bind(interval_seconds as f64)
    .fetch_all(pool)
    .await
}

pub async fn created_at_ms(pool: &PgPool, connection_id: Uuid) -> Result<u64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT FROM coinbase_connections WHERE id = $1")
        .bind(connection_id)
        .fetch_one(pool)
        .await?;
    Ok(row.0 as u64)
}

async fn insert_fills(pool: &PgPool, connection_id: Uuid, fills: &[Fill]) -> Result<(), sqlx::Error> {
    for fill in fills {
        sqlx::query(
            "INSERT INTO coinbase_fills
                (connection_id, entry_id, trade_id, order_id, product_id, is_buy, price, size, size_in_quote, commission, time_ms)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT (connection_id, entry_id) DO NOTHING",
        )
        .bind(connection_id)
        .bind(&fill.entry_id)
        .bind(&fill.trade_id)
        .bind(&fill.order_id)
        .bind(&fill.product_id)
        .bind(fill.is_buy)
        .bind(&fill.price)
        .bind(&fill.size)
        .bind(fill.size_in_quote)
        .bind(&fill.commission)
        .bind(fill.trade_time_ms as i64)
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[derive(sqlx::FromRow)]
struct FillRow {
    entry_id: String,
    trade_id: String,
    order_id: String,
    product_id: String,
    is_buy: bool,
    price: String,
    size: String,
    size_in_quote: bool,
    commission: String,
    time_ms: i64,
    reconciled: bool,
}

impl FillRow {
    fn into_fill(self) -> (Fill, bool) {
        (
            Fill {
                entry_id: self.entry_id,
                trade_id: self.trade_id,
                order_id: self.order_id,
                product_id: self.product_id,
                is_buy: self.is_buy,
                price: self.price,
                size: self.size,
                size_in_quote: self.size_in_quote,
                commission: self.commission,
                trade_time_ms: self.time_ms as u64,
            },
            self.reconciled,
        )
    }
}

/// Every stored fill, oldest first, with whether it is already reconciled.
async fn stored_fills(pool: &PgPool, connection_id: Uuid) -> Result<Vec<(Fill, bool)>, sqlx::Error> {
    let rows: Vec<FillRow> = sqlx::query_as(
        "SELECT entry_id, trade_id, order_id, product_id, is_buy, price, size, size_in_quote, commission, time_ms, reconciled
         FROM coinbase_fills WHERE connection_id = $1 ORDER BY time_ms, entry_id",
    )
    .bind(connection_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(FillRow::into_fill).collect())
}

async fn latest_snapshot(pool: &PgPool, connection_id: Uuid) -> Result<Option<BTreeMap<String, Decimal>>, sqlx::Error> {
    let row: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT balances FROM coinbase_balance_snapshots WHERE connection_id = $1 ORDER BY taken_at_ms DESC LIMIT 1",
    )
    .bind(connection_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(json,)| {
        json.as_object()
            .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), Decimal::from_str(v.as_str()?).ok()?))).collect())
            .unwrap_or_default()
    }))
}

pub async fn stored_flow_count(pool: &PgPool, connection_id: Uuid) -> Result<i64, sqlx::Error> {
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM coinbase_inferred_flows WHERE connection_id = $1")
        .bind(connection_id)
        .fetch_one(pool)
        .await?;
    Ok(count)
}

pub async fn stored_fill_count(pool: &PgPool, connection_id: Uuid) -> Result<i64, sqlx::Error> {
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM coinbase_fills WHERE connection_id = $1")
        .bind(connection_id)
        .fetch_one(pool)
        .await?;
    Ok(count)
}

// ---------- reconciliation (pure) ---------------------------------------

/// `(base, quote)` of a Coinbase product id (`BTC-USD`).
fn split_product(product_id: &str) -> Option<(&str, &str)> {
    product_id.split_once('-')
}

/// A fill's filled amount in base units and its quote value. Coinbase can
/// report `size` in quote units for orders placed that way.
fn fill_amounts(fill: &Fill) -> Option<(Decimal, Decimal)> {
    let price = Decimal::from_str(&fill.price).ok()?;
    let size = Decimal::from_str(&fill.size).ok()?;
    if price.is_zero() {
        return None;
    }
    Some(if fill.size_in_quote { (size / price, size) } else { (size, size * price) })
}

/// What one fill does to the balances: the base asset and the quote asset
/// (which also pays the fee).
pub fn fill_effects(fill: &Fill) -> Vec<(String, Decimal)> {
    let (Some((base, quote)), Some((base_amount, quote_amount)), Ok(fee)) =
        (split_product(&fill.product_id), fill_amounts(fill), Decimal::from_str(&fill.commission))
    else {
        return Vec::new();
    };
    if fill.is_buy {
        vec![(base.to_string(), base_amount), (quote.to_string(), -(quote_amount + fee))]
    } else {
        vec![(base.to_string(), -base_amount), (quote.to_string(), quote_amount - fee)]
    }
}

/// The transfers hiding between two snapshots: `now − before − (what the
/// fills explain)` per asset, ignoring rounding noise. Positive is a
/// deposit, negative a withdrawal.
pub fn infer_transfers(
    before: &BTreeMap<String, Decimal>,
    now: &BTreeMap<String, Decimal>,
    fills: &[Fill],
) -> BTreeMap<String, Decimal> {
    let mut explained: BTreeMap<String, Decimal> = BTreeMap::new();
    for fill in fills {
        for (asset, change) in fill_effects(fill) {
            *explained.entry(asset).or_default() += change;
        }
    }
    let assets: BTreeSet<&String> = before.keys().chain(now.keys()).chain(explained.keys()).collect();
    let mut transfers = BTreeMap::new();
    for asset in assets {
        let change = now.get(asset).copied().unwrap_or_default() - before.get(asset).copied().unwrap_or_default();
        let external = change - explained.get(asset).copied().unwrap_or_default();
        let dust = Decimal::from_str(if matches!(asset.as_str(), "USD" | "USDC" | "USDT" | "EUR" | "GBP") { DUST_QUOTE } else { DUST_OTHER }).unwrap();
        if external.abs() > dust {
            transfers.insert(asset.clone(), external);
        }
    }
    transfers
}

// ---------- syncing -----------------------------------------------------

fn totals(balances: &[exchange_core::AccountBalance]) -> BTreeMap<String, Decimal> {
    let mut totals = BTreeMap::new();
    for balance in balances {
        let free = Decimal::from_str(&balance.free).unwrap_or_default();
        let locked = Decimal::from_str(&balance.locked).unwrap_or_default();
        let total = free + locked;
        if !total.is_zero() {
            *totals.entry(balance.asset.clone()).or_default() += total;
        }
    }
    totals
}

/// Stores the transfers found, the new snapshot, and marks every fill as
/// accounted for.
async fn record(
    pool: &PgPool,
    connection_id: Uuid,
    now_ms: u64,
    balances: &BTreeMap<String, Decimal>,
    flows: &BTreeMap<String, Decimal>,
) -> Result<(), sqlx::Error> {
    for (asset, amount) in flows {
        sqlx::query("INSERT INTO coinbase_inferred_flows (connection_id, at_ms, asset, amount) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING")
            .bind(connection_id)
            .bind(now_ms as i64)
            .bind(asset)
            .bind(amount.to_string())
            .execute(pool)
            .await?;
    }
    let snapshot: BTreeMap<String, String> = balances.iter().map(|(a, v)| (a.clone(), v.to_string())).collect();
    sqlx::query("INSERT INTO coinbase_balance_snapshots (connection_id, taken_at_ms, balances) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
        .bind(connection_id)
        .bind(now_ms as i64)
        .bind(serde_json::to_value(snapshot).unwrap_or_default())
        .execute(pool)
        .await?;
    sqlx::query("UPDATE coinbase_fills SET reconciled = true WHERE connection_id = $1")
        .bind(connection_id)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE coinbase_connections SET last_synced_at = now() WHERE id = $1")
        .bind(connection_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Brings the stored view up to date: new fills, and — once a fill-free
/// moment allows it — the transfers found since the last snapshot.
/// Blocking HTTP, so callers run it under `spawn_blocking`'s async wrapper
/// [`sync`].
pub async fn sync(pool: &PgPool, connection: &CoinbaseConnection, client: std::sync::Arc<CoinbaseClient>, now_ms: u64) -> Result<(), String> {
    let created_at = created_at_ms(pool, connection.id).await.map_err(|e| e.to_string())?;

    let fetch_client = client.clone();
    let (balances, fills) = tokio::task::spawn_blocking(move || {
        let balances = fetch_client.balances().map_err(|e| e.to_string())?;
        let fills = fetch_client.fills_since(created_at).map_err(|e| e.to_string())?;
        Ok::<_, String>((totals(&balances), fills))
    })
    .await
    .map_err(|e| e.to_string())??;

    insert_fills(pool, connection.id, &fills).await.map_err(|e| e.to_string())?;
    let stored = stored_fills(pool, connection.id).await.map_err(|e| e.to_string())?;
    let previous = latest_snapshot(pool, connection.id).await.map_err(|e| e.to_string())?;

    match previous {
        // The very first look at the account: what it holds now is the
        // starting point, and any fill already in it is part of that.
        None => record(pool, connection.id, now_ms, &balances, &BTreeMap::new()).await.map_err(|e| e.to_string()),
        Some(before) => {
            // A fill within moments of "now" may not be in the balances yet
            // (or may be, without us knowing) — wait for the next look
            // rather than invent a transfer.
            if stored.iter().any(|(f, _)| f.trade_time_ms + SETTLE_MS > now_ms) {
                return Ok(());
            }
            let unreconciled: Vec<Fill> = stored.into_iter().filter(|(_, done)| !done).map(|(f, _)| f).collect();
            let transfers = infer_transfers(&before, &balances, &unreconciled);
            record(pool, connection.id, now_ms, &balances, &transfers).await.map_err(|e| e.to_string())
        }
    }
}

// ---------- what the engine reads ---------------------------------------

/// A stable numeric id for a Coinbase identifier (the engine's `Trade` id
/// is a number; Coinbase's are strings).
fn stable_id(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325u64, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100000001b3))
}

/// Fills since the connection, as the trades the engine understands. A
/// fill's fee is charged in the quote currency.
pub async fn trades(pool: &PgPool, connection_id: Uuid) -> Result<Vec<Trade>, sqlx::Error> {
    Ok(stored_fills(pool, connection_id)
        .await?
        .into_iter()
        .filter_map(|(fill, _)| {
            let (_, quote) = split_product(&fill.product_id)?;
            let (base_amount, _) = fill_amounts(&fill)?;
            Some(Trade {
                symbol: fill.product_id.clone(),
                id: stable_id(&fill.entry_id),
                order_id: stable_id(&fill.order_id),
                price: fill.price.clone(),
                qty: base_amount.to_string(),
                commission: fill.commission.clone(),
                commission_asset: quote.to_string(),
                time_ms: fill.trade_time_ms,
                is_buyer: fill.is_buy,
            })
        })
        .collect())
}

/// Inferred deposits and withdrawals, as the flows the engine understands.
pub async fn flows(pool: &PgPool, connection_id: Uuid) -> Result<Vec<NormalizedFlow>, sqlx::Error> {
    let rows: Vec<(i64, String, String)> =
        sqlx::query_as("SELECT at_ms, asset, amount FROM coinbase_inferred_flows WHERE connection_id = $1 ORDER BY at_ms, asset")
            .bind(connection_id)
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(at_ms, asset, amount)| {
            let amount = Decimal::from_str(&amount).ok()?;
            let deposit = amount.is_sign_positive();
            let leg = if deposit { AssetLeg::credit(asset.clone(), amount.abs().to_string()) } else { AssetLeg::debit(asset.clone(), amount.abs().to_string()) };
            Some(NormalizedFlow {
                source_namespace: if deposit { FlowFamily::Deposit } else { FlowFamily::Withdrawal },
                source_id: format!("inferred:{at_ms}:{asset}"),
                economic_time_ms: at_ms as u64,
                kind: "inferred from balance reconciliation".to_string(),
                legs: vec![leg],
                fee: None,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(value: &str) -> Decimal {
        Decimal::from_str(value).unwrap()
    }
    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, Decimal> {
        pairs.iter().map(|(a, v)| (a.to_string(), d(v))).collect()
    }
    fn fill(product: &str, buy: bool, price: &str, size: &str, fee: &str) -> Fill {
        Fill {
            entry_id: format!("{product}{price}{size}"),
            trade_id: "t".into(),
            order_id: "o".into(),
            product_id: product.into(),
            is_buy: buy,
            price: price.into(),
            size: size.into(),
            size_in_quote: false,
            commission: fee.into(),
            trade_time_ms: 1,
        }
    }

    #[test]
    fn a_buy_moves_base_up_and_pays_quote_plus_fee() {
        let effects: BTreeMap<_, _> = fill_effects(&fill("BTC-USD", true, "50000", "0.01", "2.5")).into_iter().collect();
        assert_eq!(effects["BTC"], d("0.01"));
        assert_eq!(effects["USD"], d("-502.5")); // 500 + 2.5 fee
    }

    #[test]
    fn a_sell_moves_base_down_and_receives_quote_minus_fee() {
        let effects: BTreeMap<_, _> = fill_effects(&fill("ETH-USD", false, "2000", "0.5", "1")).into_iter().collect();
        assert_eq!(effects["ETH"], d("-0.5"));
        assert_eq!(effects["USD"], d("999")); // 1000 - 1 fee
    }

    #[test]
    fn a_size_reported_in_quote_units_is_converted_to_base() {
        let mut f = fill("BTC-USD", true, "50000", "500", "0");
        f.size_in_quote = true;
        let effects: BTreeMap<_, _> = fill_effects(&f).into_iter().collect();
        assert_eq!(effects["BTC"], d("0.01"));
        assert_eq!(effects["USD"], d("-500"));
    }

    #[test]
    fn trading_alone_is_never_mistaken_for_a_transfer() {
        let before = map(&[("USD", "1000"), ("BTC", "0")]);
        let fills = [fill("BTC-USD", true, "50000", "0.01", "2.5")];
        let now = map(&[("USD", "497.5"), ("BTC", "0.01")]);
        assert!(infer_transfers(&before, &now, &fills).is_empty());
    }

    #[test]
    fn money_appearing_with_no_trade_is_a_deposit_and_disappearing_a_withdrawal() {
        let before = map(&[("USD", "100"), ("BTC", "1")]);
        let now = map(&[("USD", "600"), ("BTC", "0.4")]);
        let transfers = infer_transfers(&before, &now, &[]);
        assert_eq!(transfers["USD"], d("500"));
        assert_eq!(transfers["BTC"], d("-0.6"));
    }

    #[test]
    fn a_deposit_made_alongside_trading_is_still_found() {
        let before = map(&[("USD", "1000")]);
        let fills = [fill("BTC-USD", true, "50000", "0.01", "0")]; // spends 500
        let now = map(&[("USD", "1500"), ("BTC", "0.01")]); // 1000 - 500 + 1000 deposited
        let transfers = infer_transfers(&before, &now, &fills);
        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers["USD"], d("1000"));
    }

    #[test]
    fn rounding_dust_is_not_a_transfer() {
        let before = map(&[("USD", "100"), ("SOL", "1")]);
        let now = map(&[("USD", "100.004"), ("SOL", "1.000000004")]);
        assert!(infer_transfers(&before, &now, &[]).is_empty());
    }

    #[test]
    fn coinbase_identifiers_get_stable_distinct_numeric_ids() {
        assert_eq!(stable_id("abc"), stable_id("abc"));
        assert_ne!(stable_id("abc"), stable_id("abd"));
    }
}
