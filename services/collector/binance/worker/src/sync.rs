//! One sync cycle: fetch real data via [`binance_live_client::LiveClient`]
//! and persist it via [`crate::db`]. The HTTP client is blocking (see
//! its own doc comment), so every call into it from an async context
//! goes through [`tokio::task::spawn_blocking`].

use binance_catalog::CatalogSnapshot;
use binance_flows::NormalizedFlow;
use binance_live_client::{LiveClient, LiveClientError};
use binance_trades::Trade;
use collector_credentials::Environment;
use std::collections::BTreeSet;
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

/// How far back Simple Earn rewards are read the first time.
const FIRST_EARN_LOOKBACK_MS: u64 = 90 * 24 * 60 * 60 * 1000;
/// Rewards are read again from this long before the last one stored, so a payment that
/// reached Binance's records late is still picked up (each is stored once).
const EARN_OVERLAP_MS: u64 = 2 * 24 * 60 * 60 * 1000;

/// The currencies a held asset is usually bought with.
const QUOTES: [&str; 6] = ["USDT", "USDC", "FDUSD", "BTC", "ETH", "BNB"];

/// Which markets to read trades from. Binance only lists trades one market at a time, so
/// what is read is: the markets chosen at connection, the ones already traded on, and — so a
/// purchase made after connecting is never missed — every market of an asset the account
/// holds (in spot or Earn) against the usual quote currencies, where Binance lists it.
pub fn symbols_to_read(chosen: &[String], traded: &[String], held_assets: &BTreeSet<String>, listed: &BTreeSet<String>) -> BTreeSet<String> {
    let mut symbols: BTreeSet<String> = chosen.iter().chain(traded).cloned().collect();
    for asset in held_assets {
        for quote in QUOTES {
            let market = format!("{asset}{quote}");
            if asset != quote && listed.contains(&market) {
                symbols.insert(market);
            }
        }
    }
    symbols
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
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
    traded: Vec<String>,
    earn_from_ms: Option<u64>,
) -> Result<RawFetch, LiveClientError> {
    let mut client = LiveClient::new(api_key, api_secret, Environment::Production);
    client.ensure_read_only()?;

    let catalog = client.fetch_exchange_info().ok();
    let held: BTreeSet<String> = client
        .fetch_balances_with_earn()?
        .into_iter()
        .filter(|b| b.free.parse::<f64>().unwrap_or(0.0) + b.locked.parse::<f64>().unwrap_or(0.0) > 0.0)
        .map(|b| b.asset)
        .collect();
    let listed = catalog.as_ref().map(|c| c.symbols.clone()).unwrap_or_default();

    let mut trades = Vec::new();
    for symbol in symbols_to_read(&symbols, &traded, &held, &listed) {
        trades.extend(client.fetch_my_trades(&symbol)?);
    }

    let mut flows = client.fetch_deposits()?;
    flows.extend(client.fetch_withdrawals()?);
    // What Simple Earn (and staking through it) paid: performance, not money put in.
    let now = now_ms();
    let from = earn_from_ms.map(|t| t.saturating_sub(EARN_OVERLAP_MS)).unwrap_or_else(|| now.saturating_sub(FIRST_EARN_LOOKBACK_MS));
    flows.extend(client.fetch_earn_rewards(from, now)?);

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
    let earn_from_ms = db::latest_earn_reward_ms(pool, connection_id).await?;
    let traded = db::traded_symbols(pool, connection_id).await?;
    let fetch = tokio::task::spawn_blocking(move || fetch_blocking(api_key, api_secret, symbols, traded, earn_from_ms)).await??;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_held_asset_is_read_on_every_listed_market_with_a_usual_quote() {
        let listed = set(&["ZAMAUSDT", "ZAMABTC", "ZAMAXRP", "BTCUSDT", "BTCFDUSD"]);
        let read = symbols_to_read(&[], &[], &set(&["ZAMA", "BTC"]), &listed);
        assert_eq!(read, set(&["ZAMAUSDT", "ZAMABTC", "BTCUSDT", "BTCFDUSD"]));
    }

    #[test]
    fn markets_chosen_or_already_traded_stay_read_though_nothing_is_held() {
        let read = symbols_to_read(&["ETHUSDT".to_string()], &["SOLUSDT".to_string()], &set(&[]), &set(&[]));
        assert_eq!(read, set(&["ETHUSDT", "SOLUSDT"]));
    }

    #[test]
    fn an_asset_is_never_paired_with_itself() {
        assert!(symbols_to_read(&[], &[], &set(&["USDT"]), &set(&["USDTUSDT"])).is_empty());
    }
}
