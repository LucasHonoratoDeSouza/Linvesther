//! On-chain wallet connections: storage, syncing, and turning what a wallet did into the flows
//! the performance engine understands.
//!
//! A wallet has no exchange behind it, only an address on public chains. Each sync reads, per
//! network, the transfers since the last one up to a block old enough to be final, and the
//! balances as of that same block. What the transfers do not explain shows up as a difference
//! between balances and is recorded as a flow, so the history always adds up to what the wallet
//! actually held. The address is stored encrypted and bound to the account, and never leaves this
//! module in a message or a log.

use crate::crypto::{credential_context, decrypt, EncryptedField, MasterKey};
use binance_flows::{AssetLeg, FlowFamily, NormalizedFlow};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;
use wallet_client::snapshot::{read_chain, unexplained, ReadRequest, Reading};
use wallet_client::{check_coverage, Chain, ChainReader, LiveReader, PriceProvider, WalletMarket};
use wallet_client::{Endpoint, Explorer, Rpc};

/// Explorers and nodes limit how often they may be asked, and reading a wallet costs many calls,
/// so a connection is synced by one caller at a time and not again within this long unless asked.
const MIN_SYNC_GAP: Duration = Duration::from_secs(60);

#[derive(Debug, sqlx::FromRow)]
pub struct WalletConnection {
    pub id: Uuid,
    pub account_id: String,
    pub owner_address: String,
    pub encrypted_address: Vec<u8>,
    pub nonce_address: Vec<u8>,
    pub label: Option<String>,
    pub status: String,
    pub last_synced_at: Option<DateTime<Utc>>,
}

const COLUMNS: &str = "id, account_id, owner_address, encrypted_address, nonce_address, label, status, last_synced_at";

/// The address this connection reads, lower case.
pub fn stored_address(key: &MasterKey, connection: &WalletConnection) -> Result<String, String> {
    let nonce: [u8; 12] = connection.nonce_address.clone().try_into().map_err(|_| "corrupt stored address")?;
    decrypt(key, &connection.encrypted_address, &nonce, &credential_context("wallet", &connection.account_id, "address")).map(|a| a.to_lowercase()).map_err(|e| e.to_string())
}

/// The identity an account belongs to: the address in front of an account id
/// (`0xowner` or `0xowner_name`), lower case.
pub fn owner_of(account_id: &str) -> Option<String> {
    let owner = account_id.get(..42)?;
    let valid = owner.starts_with("0x") && owner[2..].bytes().all(|b| b.is_ascii_hexdigit());
    (valid && (account_id.len() == 42 || account_id.as_bytes()[42] == b'_')).then(|| owner.to_lowercase())
}

/// Whether `text` is a well-formed address.
pub fn is_address(text: &str) -> bool {
    text.len() == 42 && text.starts_with("0x") && text[2..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// `text` with the address hidden, for anything that may be shown or logged.
pub fn redact(text: &str, address: &str) -> String {
    let lower = text.to_lowercase();
    let needle = address.to_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    while let Some(at) = lower[from..].find(&needle) {
        out.push_str(&text[from..from + at]);
        out.push_str("[address]");
        from += at + needle.len();
    }
    out.push_str(&text[from..]);
    out
}

// ---------- settings ----------------------------------------------------

/// What this deployment reads wallets with. Explorers answer only a handful of calls an hour
/// without a key, so a key is required; the networks and the nodes can be chosen.
pub struct WalletSettings {
    pub chains: Vec<&'static Chain>,
    etherscan_key: Option<String>,
    blockscout_key: Option<String>,
    rpc_urls: BTreeMap<u64, String>,
}

impl WalletSettings {
    /// From `WALLET_CHAINS` (network ids, default the largest), `ETHERSCAN_API_KEY`,
    /// `BLOCKSCOUT_API_KEY` (each may be given as `NAME_FILE`) and `WALLET_RPC_URL_<id>`.
    pub fn from_env() -> Result<Self, String> {
        let chains = wallet_client::chains::parse_enabled(&std::env::var("WALLET_CHAINS").unwrap_or_default()).map_err(|e| format!("WALLET_CHAINS: {e}"))?;
        let rpc_urls = chains.iter().filter_map(|c| std::env::var(format!("WALLET_RPC_URL_{}", c.id)).ok().filter(|u| !u.trim().is_empty()).map(|u| (c.id, u))).collect();
        Ok(WalletSettings {
            chains,
            etherscan_key: crate::secrets::read_secret("ETHERSCAN_API_KEY")?,
            blockscout_key: crate::secrets::read_secret("BLOCKSCOUT_API_KEY")?,
            rpc_urls,
        })
    }

    /// Settings for a test or another caller.
    pub fn new(chains: Vec<&'static Chain>, etherscan_key: Option<String>, blockscout_key: Option<String>) -> Self {
        WalletSettings { chains, etherscan_key, blockscout_key, rpc_urls: BTreeMap::new() }
    }

    fn endpoint(&self, chain: &Chain) -> Result<Endpoint, String> {
        chain.endpoint(self.etherscan_key.as_deref(), self.blockscout_key.as_deref()).map_err(|e| e.to_string())
    }
}

/// Where a network's reader comes from: the real explorer and node, or a synthetic chain in a test.
pub trait ReaderSource: Send + Sync {
    fn reader(&self, chain: &'static Chain) -> Result<Box<dyn ChainReader + Send>, String>;
}

impl ReaderSource for WalletSettings {
    fn reader(&self, chain: &'static Chain) -> Result<Box<dyn ChainReader + Send>, String> {
        let node = self.rpc_urls.get(&chain.id).map(String::as_str).unwrap_or(chain.rpc_url);
        Ok(Box::new(LiveReader { explorer: Explorer::new(self.endpoint(chain)?), node: Rpc::new(node) }))
    }
}

// ---------- storage ----------------------------------------------------

/// Adds the connection with its address already encrypted. Refuses an address the same owner
/// has connected before: the address is compared after decrypting the owner's own connections,
/// under a lock, so two requests cannot both pass.
pub async fn insert_connection(pool: &PgPool, key: &MasterKey, account_id: &str, address: &str, label: Option<&str>) -> Result<Uuid, String> {
    let owner = owner_of(account_id).ok_or("the account id does not begin with an owner address")?;
    let encrypted: EncryptedField = crate::crypto::encrypt(key, &address.to_lowercase(), &credential_context("wallet", account_id, "address")).map_err(|e| e.to_string())?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))").bind(format!("wallet-owner:{owner}")).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    let existing: Vec<WalletConnection> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM wallet_connections WHERE owner_address = $1 AND account_id <> $2")).bind(&owner).bind(account_id).fetch_all(&mut *tx).await.map_err(|e| e.to_string())?;
    for other in &existing {
        if stored_address(key, other).is_ok_and(|a| a == address.to_lowercase()) {
            return Err("this wallet is already connected".to_string());
        }
    }
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO wallet_connections (account_id, owner_address, encrypted_address, nonce_address, label)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (account_id) DO UPDATE SET
            encrypted_address = EXCLUDED.encrypted_address,
            nonce_address = EXCLUDED.nonce_address,
            status = 'active'
         RETURNING id",
    )
    .bind(account_id)
    .bind(&owner)
    .bind(&encrypted.ciphertext)
    .bind(encrypted.nonce.as_slice())
    .bind(label)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(id)
}

/// Deletes the connection and, by cascade, the address and everything collected for it.
pub async fn delete_connection(pool: &PgPool, connection_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM wallet_connections WHERE id = $1").bind(connection_id).execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_label(pool: &PgPool, connection_id: Uuid, label: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE wallet_connections SET label = $2 WHERE id = $1").bind(connection_id).bind(label).execute(pool).await?;
    Ok(())
}

pub async fn find_by_account(pool: &PgPool, account_id: &str) -> Result<Option<WalletConnection>, sqlx::Error> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM wallet_connections WHERE account_id = $1")).bind(account_id).fetch_optional(pool).await
}

pub async fn owned_by(pool: &PgPool, owner_address: &str) -> Result<Vec<crate::db::OwnedConnection>, sqlx::Error> {
    sqlx::query_as(
        "SELECT account_id, label, (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS connected_at_ms, status, 'wallet' AS broker
         FROM wallet_connections WHERE owner_address = lower($1) ORDER BY created_at",
    )
    .bind(owner_address)
    .fetch_all(pool)
    .await
}

pub async fn due_for_sync(pool: &PgPool, interval_seconds: i64) -> Result<Vec<WalletConnection>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM wallet_connections
         WHERE status = 'active' AND (last_synced_at IS NULL OR last_synced_at < now() - make_interval(secs => $1))"
    ))
    .bind(interval_seconds as f64)
    .fetch_all(pool)
    .await
}

pub async fn created_at_ms(pool: &PgPool, connection_id: Uuid) -> Result<u64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT FROM wallet_connections WHERE id = $1").bind(connection_id).fetch_one(pool).await?;
    Ok(row.0 as u64)
}

#[derive(Debug, sqlx::FromRow)]
pub struct ChainState {
    pub chain_id: i64,
    pub synced_block: i64,
    pub transfers_incomplete: bool,
}

pub async fn chain_states(pool: &PgPool, connection_id: Uuid) -> Result<Vec<ChainState>, sqlx::Error> {
    sqlx::query_as("SELECT chain_id, synced_block, transfers_incomplete FROM wallet_chain_state WHERE connection_id = $1 ORDER BY chain_id").bind(connection_id).fetch_all(pool).await
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AnchorRow {
    pub chain_id: i64,
    pub asset: String,
    pub quantity: String,
    pub symbol: String,
    pub decimals: i32,
    pub unrepresentable: bool,
}

pub async fn anchors(pool: &PgPool, connection_id: Uuid) -> Result<Vec<AnchorRow>, sqlx::Error> {
    sqlx::query_as("SELECT chain_id, asset, quantity, symbol, decimals, unrepresentable FROM wallet_anchors WHERE connection_id = $1 ORDER BY chain_id, asset").bind(connection_id).fetch_all(pool).await
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EffectRow {
    pub chain_id: i64,
    pub tx_hash: String,
    pub asset: String,
    pub kind: String,
    pub time_ms: i64,
    pub delta: String,
}

pub async fn effects(pool: &PgPool, connection_id: Uuid) -> Result<Vec<EffectRow>, sqlx::Error> {
    sqlx::query_as("SELECT chain_id, tx_hash, asset, kind, time_ms, delta FROM wallet_effects WHERE connection_id = $1 ORDER BY time_ms, chain_id, tx_hash, asset").bind(connection_id).fetch_all(pool).await
}

pub async fn effect_count(pool: &PgPool, connection_id: Uuid) -> Result<i64, sqlx::Error> {
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM wallet_effects WHERE connection_id = $1").bind(connection_id).fetch_one(pool).await?;
    Ok(count)
}

// ---------- syncing -----------------------------------------------------

fn lock_key(connection_id: Uuid) -> i64 {
    i64::from_le_bytes(connection_id.as_bytes()[8..].try_into().expect("a UUID has 16 bytes"))
}

/// Reads every network the connection follows and keeps what it finds. With `first`, every
/// network in `chains` is read for the first time (this is the connection itself); otherwise the
/// networks are the ones already followed. A network that fails does not stop the others from
/// being kept, but the call reports it.
#[allow(clippy::too_many_arguments)]
pub async fn sync(
    pool: &PgPool,
    connection: &WalletConnection,
    address: &str,
    chains: &[&'static Chain],
    readers: Arc<dyn ReaderSource>,
    prices: Arc<dyn PriceProvider>,
    now_ms: u64,
    force: bool,
) -> Result<(), String> {
    let mut guard = pool.acquire().await.map_err(|e| e.to_string())?;
    let (acquired,): (bool,) = sqlx::query_as("SELECT pg_try_advisory_lock($1)").bind(lock_key(connection.id)).fetch_one(&mut *guard).await.map_err(|e| e.to_string())?;
    if !acquired {
        return Ok(());
    }
    let result = sync_locked(pool, connection, address, chains, readers, prices, now_ms, force).await;
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)").bind(lock_key(connection.id)).execute(&mut *guard).await;
    result.map_err(|e| redact(&e, address))
}

#[allow(clippy::too_many_arguments)]
async fn sync_locked(
    pool: &PgPool,
    connection: &WalletConnection,
    address: &str,
    chains: &[&'static Chain],
    readers: Arc<dyn ReaderSource>,
    prices: Arc<dyn PriceProvider>,
    now_ms: u64,
    force: bool,
) -> Result<(), String> {
    if !force {
        let (recent,): (bool,) = sqlx::query_as("SELECT coalesce(last_synced_at > now() - make_interval(secs => $2), false) FROM wallet_connections WHERE id = $1")
            .bind(connection.id)
            .bind(MIN_SYNC_GAP.as_secs() as f64)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;
        if recent {
            return Ok(());
        }
    }
    let states: BTreeMap<i64, ChainState> = chain_states(pool, connection.id).await.map_err(|e| e.to_string())?.into_iter().map(|s| (s.chain_id, s)).collect();
    let stored_anchors = anchors(pool, connection.id).await.map_err(|e| e.to_string())?;
    let stored_effects = effects(pool, connection.id).await.map_err(|e| e.to_string())?;

    let mut failures = Vec::new();
    for chain in chains {
        let state = states.get(&(chain.id as i64));
        let previous: BTreeMap<String, (Decimal, u32)> = stored_anchors
            .iter()
            .filter(|a| a.chain_id == chain.id as i64 && !a.unrepresentable)
            .filter_map(|a| Some((a.asset.clone(), (Decimal::from_str(&a.quantity).ok()?, a.decimals as u32))))
            .collect();
        let mut known: BTreeSet<String> = stored_anchors.iter().filter(|a| a.chain_id == chain.id as i64).map(|a| a.asset.clone()).collect();
        known.extend(stored_effects.iter().filter(|e| e.chain_id == chain.id as i64).map(|e| e.asset.clone()));
        let after = state.map(|s| s.synced_block as u64);

        let (readers, prices, address_owned) = (readers.clone(), prices.clone(), address.to_string());
        let chain_ref: &'static Chain = chain;
        let reading = tokio::task::spawn_blocking(move || {
            let reader = readers.reader(chain_ref)?;
            read_chain(reader.as_ref(), prices.as_ref(), &ReadRequest { chain: chain_ref, address: &address_owned, after_block: after, known_assets: &known }).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?;
        match reading {
            Ok(Some(reading)) => {
                let first_reading = state.is_none();
                if let Err(e) = persist(pool, connection.id, chain, &reading, (!first_reading).then_some(&previous), now_ms).await {
                    failures.push(format!("{}: {e}", chain.name));
                }
            }
            Ok(None) => {}
            Err(e) => failures.push(format!("{}: {e}", chain.name)),
        }
    }
    if failures.is_empty() {
        sqlx::query("UPDATE wallet_connections SET last_synced_at = now() WHERE id = $1").bind(connection.id).execute(pool).await.map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

/// Keeps one network's reading in a single transaction: the effects, the difference between
/// balances that they do not explain (when there was an earlier reading), the new balances and
/// the block reached.
async fn persist(pool: &PgPool, connection_id: Uuid, chain: &Chain, reading: &Reading, previous: Option<&BTreeMap<String, (Decimal, u32)>>, now_ms: u64) -> Result<(), String> {
    let chain_id = chain.id as i64;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    for effect in &reading.confirmed {
        let kind = match effect.kind {
            wallet_client::EffectKind::Transfer => "transfer",
            wallet_client::EffectKind::Gas => "gas",
        };
        sqlx::query("INSERT INTO wallet_effects (connection_id, chain_id, tx_hash, asset, kind, block, time_ms, delta) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT DO NOTHING")
            .bind(connection_id)
            .bind(chain_id)
            .bind(&effect.tx_hash)
            .bind(&effect.asset)
            .bind(kind)
            .bind(effect.block as i64)
            .bind(effect.time_ms as i64)
            .bind(effect.delta.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    if let Some(previous) = previous {
        let before: BTreeMap<String, Decimal> = previous.iter().map(|(asset, (quantity, _))| (asset.clone(), *quantity)).collect();
        let decimals_of = |asset: &str| -> u32 {
            if asset.ends_with(":native") {
                18
            } else {
                reading.tokens.get(asset).map(|t| t.decimals).or_else(|| previous.get(asset).map(|(_, d)| *d)).unwrap_or(18)
            }
        };
        for (asset, difference) in unexplained(&before, &reading.confirmed, &reading.anchor, &decimals_of) {
            sqlx::query("INSERT INTO wallet_effects (connection_id, chain_id, tx_hash, asset, kind, block, time_ms, delta) VALUES ($1, $2, $3, $4, 'unexplained', $5, $6, $7) ON CONFLICT DO NOTHING")
                .bind(connection_id)
                .bind(chain_id)
                .bind(format!("unexplained:{}", reading.end))
                .bind(asset)
                .bind(reading.end as i64)
                .bind(now_ms as i64)
                .bind(difference.to_string())
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    sqlx::query("DELETE FROM wallet_anchors WHERE connection_id = $1 AND chain_id = $2").bind(connection_id).bind(chain_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    for (asset, quantity) in &reading.anchor {
        let (symbol, decimals) = if asset.ends_with(":native") { (chain.native_symbol.to_string(), 18) } else { reading.tokens.get(asset).map(|t| (t.symbol.clone(), t.decimals as i32)).unwrap_or_default() };
        sqlx::query("INSERT INTO wallet_anchors (connection_id, chain_id, asset, quantity, block, symbol, decimals) VALUES ($1, $2, $3, $4, $5, $6, $7)")
            .bind(connection_id)
            .bind(chain_id)
            .bind(asset)
            .bind(quantity.to_string())
            .bind(reading.end as i64)
            .bind(symbol)
            .bind(decimals)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    for asset in &reading.unrepresentable {
        sqlx::query("INSERT INTO wallet_anchors (connection_id, chain_id, asset, quantity, block, unrepresentable) VALUES ($1, $2, $3, '0', $4, true) ON CONFLICT (connection_id, chain_id, asset) DO UPDATE SET unrepresentable = true")
            .bind(connection_id)
            .bind(chain_id)
            .bind(asset)
            .bind(reading.end as i64)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    sqlx::query(
        "INSERT INTO wallet_chain_state (connection_id, chain_id, synced_block, synced_at_ms, transfers_incomplete) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (connection_id, chain_id) DO UPDATE SET synced_block = EXCLUDED.synced_block, synced_at_ms = EXCLUDED.synced_at_ms, transfers_incomplete = EXCLUDED.transfers_incomplete",
    )
    .bind(connection_id)
    .bind(chain_id)
    .bind(reading.end as i64)
    .bind(now_ms as i64)
    .bind(reading.transfers_incomplete)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())
}

/// The networks a stored connection follows, or an error naming one that is no longer usable.
pub async fn followed_chains(pool: &PgPool, connection_id: Uuid) -> Result<Vec<&'static Chain>, String> {
    let states = chain_states(pool, connection_id).await.map_err(|e| e.to_string())?;
    states.iter().map(|s| Chain::by_id(s.chain_id as u64).ok_or_else(|| format!("network {} is no longer supported", s.chain_id))).collect()
}

// ---------- what the engine reads ---------------------------------------

fn amount(text: &str) -> Result<Decimal, String> {
    Decimal::from_str(text).map_err(|_| format!("a stored amount is unreadable: {text:?}"))
}

fn leg(asset: &str, delta: Decimal) -> AssetLeg {
    if delta.is_sign_negative() {
        AssetLeg::debit(asset, delta.abs().to_string())
    } else {
        AssetLeg::credit(asset, delta.to_string())
    }
}

/// Turns the effects into flows. A transaction that only receives is money coming in, one that only
/// sends is money going out, and one that does both (a swap) changes what is held without putting
/// money in or taking it out. Gas is a cost, not a movement of capital. A difference between
/// balances that no transfer explains is treated as money in or out: never as a gain, so a
/// missing transfer can understate performance but not invent it.
pub fn classify(rows: &[EffectRow]) -> Result<Vec<NormalizedFlow>, String> {
    let mut transactions: BTreeMap<(i64, &str), Vec<&EffectRow>> = BTreeMap::new();
    let mut flows = Vec::new();
    for row in rows {
        let delta = amount(&row.delta)?;
        match row.kind.as_str() {
            "transfer" => transactions.entry((row.chain_id, row.tx_hash.as_str())).or_default().push(row),
            "gas" => flows.push(NormalizedFlow {
                source_namespace: FlowFamily::Transfer,
                source_id: format!("gas:{}:{}", row.chain_id, row.tx_hash),
                economic_time_ms: row.time_ms as u64,
                kind: "gas".to_string(),
                legs: vec![leg(&row.asset, delta)],
                fee: None,
            }),
            "unexplained" => flows.push(NormalizedFlow {
                source_namespace: if delta.is_sign_negative() { FlowFamily::Withdrawal } else { FlowFamily::Deposit },
                source_id: format!("unexplained:{}:{}:{}", row.chain_id, row.tx_hash, row.asset),
                economic_time_ms: row.time_ms as u64,
                kind: "inferred from balance reconciliation".to_string(),
                legs: vec![leg(&row.asset, delta)],
                fee: None,
            }),
            other => return Err(format!("a stored effect has an unknown kind {other:?}")),
        }
    }
    for ((chain_id, hash), effects) in transactions {
        let mut legs = Vec::with_capacity(effects.len());
        let (mut credits, mut debits) = (false, false);
        for effect in &effects {
            let delta = amount(&effect.delta)?;
            credits |= !delta.is_sign_negative();
            debits |= delta.is_sign_negative();
            legs.push(leg(&effect.asset, delta));
        }
        let family = match (credits, debits) {
            (true, true) => FlowFamily::Convert,
            (true, false) => FlowFamily::Deposit,
            _ => FlowFamily::Withdrawal,
        };
        flows.push(NormalizedFlow {
            source_namespace: family,
            source_id: format!("tx:{chain_id}:{hash}"),
            economic_time_ms: effects.iter().map(|e| e.time_ms).min().unwrap_or_default() as u64,
            kind: family.to_string(),
            legs,
            fee: None,
        });
    }
    flows.sort_by(|a, b| (a.economic_time_ms, &a.source_id).cmp(&(b.economic_time_ms, &b.source_id)));
    Ok(flows)
}

/// Assets the wallet moved on purpose: sent, or swapped. An unpriced one of these cannot be left
/// out of the figures. One that was only ever received is not.
pub fn active_assets(rows: &[EffectRow]) -> Result<BTreeSet<String>, String> {
    let mut active = BTreeSet::new();
    let mut by_transaction: BTreeMap<(i64, &str), Vec<(&str, bool)>> = BTreeMap::new();
    for row in rows.iter().filter(|r| r.kind == "transfer") {
        by_transaction.entry((row.chain_id, row.tx_hash.as_str())).or_default().push((row.asset.as_str(), amount(&row.delta)?.is_sign_negative()));
    }
    for legs in by_transaction.values() {
        let swapped = legs.iter().any(|(_, debit)| *debit) && legs.iter().any(|(_, debit)| !*debit);
        for (asset, debit) in legs {
            if *debit || swapped {
                active.insert(asset.to_string());
            }
        }
    }
    for row in rows.iter().filter(|r| r.kind == "unexplained" || r.kind == "gas") {
        if amount(&row.delta)?.is_sign_negative() {
            active.insert(row.asset.clone());
        }
    }
    Ok(active)
}

pub async fn flows(pool: &PgPool, connection_id: Uuid) -> Result<Vec<NormalizedFlow>, String> {
    classify(&effects(pool, connection_id).await.map_err(|e| e.to_string())?)
}

/// The market the wallet is valued in, or the reason its figures cannot be given. Every asset
/// followed must be priced, except tokens that were only ever received and have no price.
/// Blocking (it asks the price source), so callers run it under `spawn_blocking`'s wrapper here.
pub async fn market(pool: &PgPool, connection_id: Uuid, prices: Arc<dyn PriceProvider>) -> Result<Arc<WalletMarket>, String> {
    let stored = anchors(pool, connection_id).await.map_err(|e| e.to_string())?;
    let rows = effects(pool, connection_id).await.map_err(|e| e.to_string())?;
    let active = active_assets(&rows)?;
    let unrepresentable: BTreeSet<String> = stored.iter().filter(|a| a.unrepresentable).map(|a| a.asset.clone()).collect();
    let followed: BTreeSet<String> = stored.iter().map(|a| a.asset.clone()).collect();
    let balances: Vec<exchange_core::AccountBalance> = stored.iter().filter(|a| !a.unrepresentable).map(|a| exchange_core::AccountBalance { asset: a.asset.clone(), free: a.quantity.clone(), locked: "0".to_string() }).collect();
    tokio::task::spawn_blocking(move || {
        let market = Arc::new(WalletMarket::new(prices, balances));
        let priced = market.priced_assets().map_err(|e| e.to_string())?;
        check_coverage(&followed, &active, &priced, &unrepresentable).map_err(|e| e.to_string())?;
        Ok(market)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(chain_id: i64, hash: &str, asset: &str, kind: &str, time_ms: i64, delta: &str) -> EffectRow {
        EffectRow { chain_id, tx_hash: hash.into(), asset: asset.into(), kind: kind.into(), time_ms, delta: delta.into() }
    }

    fn one(rows: &[EffectRow]) -> NormalizedFlow {
        let flows = classify(rows).unwrap();
        assert_eq!(flows.len(), 1, "{flows:?}");
        flows.into_iter().next().unwrap()
    }

    #[test]
    fn receiving_is_a_deposit_and_sending_a_withdrawal() {
        let deposit = one(&[row(1, "0xa", "ethereum:native", "transfer", 1000, "2.5")]);
        assert_eq!(deposit.source_namespace, FlowFamily::Deposit);
        assert_eq!(deposit.legs, vec![AssetLeg::credit("ethereum:native", "2.5")]);
        let withdrawal = one(&[row(1, "0xb", "ethereum:0xusdc", "transfer", 2000, "-100")]);
        assert_eq!(withdrawal.source_namespace, FlowFamily::Withdrawal);
        assert_eq!(withdrawal.legs, vec![AssetLeg::debit("ethereum:0xusdc", "100")]);
    }

    #[test]
    fn a_swap_is_a_conversion_not_money_in_or_out() {
        let swap = one(&[row(1, "0xs", "ethereum:0xusdc", "transfer", 3000, "-100"), row(1, "0xs", "ethereum:native", "transfer", 3000, "0.04")]);
        assert_eq!(swap.source_namespace, FlowFamily::Convert);
        assert_eq!(swap.legs.len(), 2);
    }

    #[test]
    fn gas_is_a_cost_and_never_capital_even_when_the_transaction_also_moved_funds() {
        let flows = classify(&[row(1, "0xw", "ethereum:0xusdc", "transfer", 4000, "-50"), row(1, "0xw", "ethereum:native", "gas", 4000, "-0.001")]).unwrap();
        let gas = flows.iter().find(|f| f.source_id.starts_with("gas:")).unwrap();
        assert_eq!(gas.source_namespace, FlowFamily::Transfer, "not a Deposit or Withdrawal, so it is performance");
        assert_eq!(gas.legs, vec![AssetLeg::debit("ethereum:native", "0.001")]);
        assert_eq!(flows.iter().find(|f| f.source_id.starts_with("tx:")).unwrap().source_namespace, FlowFamily::Withdrawal);
    }

    #[test]
    fn a_difference_no_transfer_explains_is_money_in_or_out_never_a_gain() {
        let up = one(&[row(8453, "unexplained:900", "base:native", "unexplained", 5000, "0.04")]);
        assert_eq!(up.source_namespace, FlowFamily::Deposit);
        let down = one(&[row(8453, "unexplained:900", "base:native", "unexplained", 5000, "-0.001")]);
        assert_eq!(down.source_namespace, FlowFamily::Withdrawal);
    }

    #[test]
    fn the_same_hash_on_two_networks_is_two_transactions() {
        let flows = classify(&[row(1, "0xsame", "ethereum:native", "transfer", 1000, "1"), row(8453, "0xsame", "base:native", "transfer", 1000, "-1")]).unwrap();
        assert_eq!(flows.len(), 2);
    }

    #[test]
    fn an_unknown_kind_or_an_unreadable_amount_fails_closed() {
        assert!(classify(&[row(1, "0x", "a", "mystery", 1, "1")]).is_err());
        assert!(classify(&[row(1, "0x", "a", "transfer", 1, "lots")]).is_err());
    }

    #[test]
    fn only_what_was_sent_or_swapped_counts_as_moved_on_purpose() {
        let rows = [
            row(1, "0x1", "ethereum:0xairdrop", "transfer", 1, "5"),
            row(1, "0x2", "ethereum:0xsold", "transfer", 2, "-3"),
            row(1, "0x3", "ethereum:0xin", "transfer", 3, "7"),
            row(1, "0x3", "ethereum:0xout", "transfer", 3, "-2"),
            row(1, "unexplained:9", "ethereum:0xleft", "unexplained", 4, "-1"),
        ];
        let active = active_assets(&rows).unwrap();
        assert!(!active.contains("ethereum:0xairdrop"), "received only");
        for moved in ["ethereum:0xsold", "ethereum:0xin", "ethereum:0xout", "ethereum:0xleft"] {
            assert!(active.contains(moved), "{moved}");
        }
    }

    #[test]
    fn an_account_id_names_its_owner_and_an_address_is_recognised() {
        let owner = "0xF493aD63385D04e5F638b3cA4672138A983Cc4c8";
        assert_eq!(owner_of(owner).as_deref(), Some(owner.to_lowercase().as_str()));
        assert_eq!(owner_of(&format!("{owner}_main")).as_deref(), Some(owner.to_lowercase().as_str()));
        assert_eq!(owner_of(&format!("{owner}x")), None);
        assert_eq!(owner_of("short"), None);
        assert!(is_address(owner) && !is_address("0x123") && !is_address(&owner.replace('0', "g")));
    }

    #[test]
    fn an_address_is_removed_from_any_text_however_it_is_written() {
        let address = "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045";
        let text = format!("bad address {address} and again {} at the end", address.to_lowercase());
        let redacted = redact(&text, address);
        assert!(!redacted.to_lowercase().contains(&address.to_lowercase()[2..]));
        assert_eq!(redacted, "bad address [address] and again [address] at the end");
    }
}
