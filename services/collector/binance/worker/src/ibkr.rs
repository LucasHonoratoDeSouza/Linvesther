//! Interactive Brokers connections: storage, syncing, and turning a
//! Flex report into performance metrics.
//!
//! Unlike Binance/Coinbase, IBKR's report already gives one total
//! account value per day (`EquitySummaryInBase`) — there is no need to
//! reconstruct NAV from trades and market prices, so this module does
//! not go through `history.rs`'s trade-driven engine at all. Trades are
//! stored only for the win-rate metric; no per-position detail
//! (average cost, unrealized P&L) is computed for IBKR accounts.
//!
//! Return between two report dates is computed as
//! `(E_t - flow_t) / E_(t-1) - 1`, i.e. a flow is assumed to land at
//! the end of the day it is reported on and to already be included in
//! that day's total — the standard approximation for daily-valued,
//! daily-flow-dated data (a true Modified Dietz calculation would need
//! each flow's intraday time, which the Flex report does not give).
//!
//! **Not verified against a real account** — see `ibkr-client`'s own
//! doc comment.
use crate::crypto::{decrypt, EncryptedField, MasterKey};
use crate::history::SeriesRange;
use chrono::NaiveDate;
use ibkr_client::{Credentials, IbkrClient};
use metrics::{self, Fill};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

const DAY_MS: i64 = 86_400_000;

#[derive(Debug, sqlx::FromRow)]
pub struct IbkrConnection {
    pub id: Uuid,
    pub account_id: String,
    pub encrypted_token: Vec<u8>,
    pub nonce_token: Vec<u8>,
    pub encrypted_query_id: Vec<u8>,
    pub nonce_query_id: Vec<u8>,
    pub label: Option<String>,
    pub status: String,
    pub last_synced_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub fn credentials(key: &MasterKey, connection: &IbkrConnection) -> Result<Credentials, String> {
    let nonce_token: [u8; 12] = connection.nonce_token.clone().try_into().map_err(|_| "corrupt stored token")?;
    let nonce_query: [u8; 12] = connection.nonce_query_id.clone().try_into().map_err(|_| "corrupt stored query id")?;
    let token = decrypt(key, &connection.encrypted_token, &nonce_token).map_err(|e| e.to_string())?;
    let query_id = decrypt(key, &connection.encrypted_query_id, &nonce_query).map_err(|e| e.to_string())?;
    Credentials::new(&token, &query_id).map_err(|e| e.to_string())
}

// ---------- storage ----------------------------------------------------

pub async fn insert_connection(pool: &PgPool, account_id: &str, token: &EncryptedField, query_id: &EncryptedField) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO ibkr_connections (account_id, encrypted_token, nonce_token, encrypted_query_id, nonce_query_id)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (account_id) DO UPDATE SET
            encrypted_token = EXCLUDED.encrypted_token,
            nonce_token = EXCLUDED.nonce_token,
            encrypted_query_id = EXCLUDED.encrypted_query_id,
            nonce_query_id = EXCLUDED.nonce_query_id,
            status = 'active'
         RETURNING id",
    )
    .bind(account_id)
    .bind(&token.ciphertext)
    .bind(token.nonce.as_slice())
    .bind(&query_id.ciphertext)
    .bind(query_id.nonce.as_slice())
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Deletes the stored credential and, by cascade, everything collected for it.
pub async fn delete_connection(pool: &PgPool, connection_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM ibkr_connections WHERE id = $1")
        .bind(connection_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_label(pool: &PgPool, connection_id: Uuid, label: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE ibkr_connections SET label = $2 WHERE id = $1").bind(connection_id).bind(label).execute(pool).await?;
    Ok(())
}

pub async fn find_by_account(pool: &PgPool, account_id: &str) -> Result<Option<IbkrConnection>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, account_id, encrypted_token, nonce_token, encrypted_query_id, nonce_query_id, label, status, last_synced_at
         FROM ibkr_connections WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
}

pub async fn created_at_ms(pool: &PgPool, connection_id: Uuid) -> Result<u64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT FROM ibkr_connections WHERE id = $1")
        .bind(connection_id)
        .fetch_one(pool)
        .await?;
    Ok(row.0 as u64)
}

pub async fn owned_by(pool: &PgPool, owner_address: &str) -> Result<Vec<crate::db::OwnedConnection>, sqlx::Error> {
    sqlx::query_as(
        "SELECT account_id, label, (EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS connected_at_ms, status, 'ibkr' AS broker
         FROM ibkr_connections
         WHERE lower(account_id) = lower($1) OR lower(account_id) LIKE lower($1) || '\\_%' ESCAPE '\\'
         ORDER BY created_at",
    )
    .bind(owner_address)
    .fetch_all(pool)
    .await
}

pub async fn due_for_sync(pool: &PgPool, interval_seconds: i64) -> Result<Vec<IbkrConnection>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, account_id, encrypted_token, nonce_token, encrypted_query_id, nonce_query_id, label, status, last_synced_at
         FROM ibkr_connections
         WHERE status = 'active'
           AND (last_synced_at IS NULL OR last_synced_at < now() - make_interval(secs => $1))",
    )
    .bind(interval_seconds as f64)
    .fetch_all(pool)
    .await
}

/// Pulls the connection's Flex report and stores whatever is new: equity
/// points, deposit/withdrawal flows, and trades — only from the
/// connection's own creation date onward, so a report covering a wider
/// period than requested still respects "measured from when you
/// connected".
pub async fn sync(pool: &PgPool, connection: &IbkrConnection, client: Arc<IbkrClient>, _now_ms: u64) -> Result<(), String> {
    let since_ms = created_at_ms(pool, connection.id).await.map_err(|e| e.to_string())?;
    let since_date = date_at_ms(since_ms as i64);

    let statement = tokio::task::spawn_blocking(move || client.fetch_report()).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;

    for point in &statement.equity {
        let Some(date) = parse_report_date(&point.report_date) else { continue };
        if date < since_date {
            continue;
        }
        sqlx::query("INSERT INTO ibkr_equity_points (connection_id, report_date, total) VALUES ($1, $2, $3) ON CONFLICT (connection_id, report_date) DO UPDATE SET total = EXCLUDED.total")
            .bind(connection.id)
            .bind(date)
            .bind(point.total)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }

    for flow in &statement.flows {
        let Some(date) = parse_report_date(&flow.report_date) else { continue };
        if date < since_date {
            continue;
        }
        sqlx::query("INSERT INTO ibkr_flows (connection_id, report_date, amount, currency) VALUES ($1, $2, $3, $4) ON CONFLICT (connection_id, report_date, amount) DO NOTHING")
            .bind(connection.id)
            .bind(date)
            .bind(flow.amount)
            .bind(&flow.currency)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }

    for (ordinal, trade) in statement.trades.iter().enumerate() {
        let Some(date) = parse_report_date(&trade.trade_date) else { continue };
        if date < since_date {
            continue;
        }
        sqlx::query(
            "INSERT INTO ibkr_trades (connection_id, trade_date, symbol, is_buy, quantity, price, ordinal) VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (connection_id, trade_date, symbol, ordinal) DO NOTHING",
        )
        .bind(connection.id)
        .bind(date)
        .bind(&trade.symbol)
        .bind(trade.is_buy)
        .bind(trade.quantity)
        .bind(trade.price)
        .bind(ordinal as i32)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    sqlx::query("UPDATE ibkr_connections SET last_synced_at = now() WHERE id = $1").bind(connection.id).execute(pool).await.map_err(|e| e.to_string())?;
    Ok(())
}

fn parse_report_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y%m%d").ok()
}

fn date_at_ms(ms: i64) -> NaiveDate {
    chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.date_naive()).unwrap_or_default()
}

fn ms_at_date(date: NaiveDate) -> i64 {
    date.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc().timestamp_millis()
}

async fn stored_equity(pool: &PgPool, connection_id: Uuid) -> Result<Vec<(NaiveDate, Decimal)>, sqlx::Error> {
    let rows: Vec<(NaiveDate, Decimal)> = sqlx::query_as("SELECT report_date, total FROM ibkr_equity_points WHERE connection_id = $1 ORDER BY report_date").bind(connection_id).fetch_all(pool).await?;
    Ok(rows)
}

async fn stored_flows(pool: &PgPool, connection_id: Uuid) -> Result<Vec<(NaiveDate, Decimal)>, sqlx::Error> {
    let rows: Vec<(NaiveDate, Decimal)> = sqlx::query_as("SELECT report_date, amount FROM ibkr_flows WHERE connection_id = $1 ORDER BY report_date").bind(connection_id).fetch_all(pool).await?;
    Ok(rows)
}

async fn stored_trades(pool: &PgPool, connection_id: Uuid) -> Result<Vec<(NaiveDate, bool, Decimal, Decimal)>, sqlx::Error> {
    let rows: Vec<(NaiveDate, bool, Decimal, Decimal)> =
        sqlx::query_as("SELECT trade_date, is_buy, quantity, price FROM ibkr_trades WHERE connection_id = $1 ORDER BY trade_date, ordinal").bind(connection_id).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn stored_equity_count(pool: &PgPool, connection_id: Uuid) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT count(*) FROM ibkr_equity_points WHERE connection_id = $1").bind(connection_id).fetch_one(pool).await?;
    Ok(row.0)
}

pub async fn stored_flow_count(pool: &PgPool, connection_id: Uuid) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT count(*) FROM ibkr_flows WHERE connection_id = $1").bind(connection_id).fetch_one(pool).await?;
    Ok(row.0)
}

// ---------- computation --------------------------------------------------

/// One TWR index point built from the stored equity/flow series.
#[derive(Debug, Clone, Copy)]
pub struct IbkrIndexPoint {
    pub time_ms: u64,
    pub nav: Decimal,
    pub index: Decimal,
}

/// Builds the day-by-day TWR index from equity points and flows — see
/// the module doc comment for the return formula. The first point in
/// the input is `index = 1` regardless of its own value.
fn build_index(equity: &[(NaiveDate, Decimal)], flows: &[(NaiveDate, Decimal)]) -> Vec<IbkrIndexPoint> {
    if equity.is_empty() {
        return Vec::new();
    }
    let mut flow_by_date: std::collections::BTreeMap<NaiveDate, Decimal> = std::collections::BTreeMap::new();
    for (date, amount) in flows {
        *flow_by_date.entry(*date).or_insert(Decimal::ZERO) += *amount;
    }

    let mut points = Vec::with_capacity(equity.len());
    let mut index = Decimal::ONE;
    let mut previous_total: Option<Decimal> = None;
    for (date, total) in equity {
        if let Some(previous) = previous_total {
            if previous > Decimal::ZERO {
                let flow = flow_by_date.get(date).copied().unwrap_or(Decimal::ZERO);
                let period_return = (*total - flow) / previous - Decimal::ONE;
                index *= Decimal::ONE + period_return;
            }
        }
        points.push(IbkrIndexPoint { time_ms: ms_at_date(*date) as u64, nav: *total, index });
        previous_total = Some(*total);
    }
    points
}

pub struct IbkrSeries {
    pub points: Vec<IbkrIndexPoint>,
    /// Always one day — IBKR's report is daily, so there is no finer
    /// resolution to draw at any range.
    pub step_ms: u64,
}

pub async fn load_series(pool: &PgPool, connection_id: Uuid, range: SeriesRange) -> Result<IbkrSeries, String> {
    let equity = stored_equity(pool, connection_id).await.map_err(|e| e.to_string())?;
    let flows = stored_flows(pool, connection_id).await.map_err(|e| e.to_string())?;
    let mut points = build_index(&equity, &flows);
    if let Some(window_ms) = range.window_ms() {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let cutoff = now_ms - window_ms as i64;
        points.retain(|p| p.time_ms as i64 >= cutoff);
    }
    Ok(IbkrSeries { points, step_ms: DAY_MS as u64 })
}

#[derive(Debug)]
pub struct IbkrPerformance {
    pub daily_nav: Vec<(u64, Decimal)>,
    pub daily_returns: Vec<Decimal>,
    pub sharpe: Result<Decimal, metrics::StatsError>,
    pub sortino: Result<Decimal, metrics::StatsError>,
    pub max_drawdown: Option<metrics::DrawdownEpisode>,
    pub cagr: Result<Decimal, metrics::CagrError>,
    pub win_rate: Option<metrics::WinRateResult>,
    pub earliest_reliable_checkpoint_ms: Option<u64>,
    /// The account's total change since the connection, in its own base
    /// currency — the only P&L figure available (no per-position
    /// breakdown for IBKR yet).
    pub total_change: Decimal,
    pub currency: String,
}

pub async fn load_performance(pool: &PgPool, connection_id: Uuid) -> Result<IbkrPerformance, String> {
    let equity = stored_equity(pool, connection_id).await.map_err(|e| e.to_string())?;
    let flows = stored_flows(pool, connection_id).await.map_err(|e| e.to_string())?;
    let trade_rows = stored_trades(pool, connection_id).await.map_err(|e| e.to_string())?;
    let currency_row: Option<(String,)> = sqlx::query_as("SELECT currency FROM ibkr_flows WHERE connection_id = $1 ORDER BY report_date LIMIT 1").bind(connection_id).fetch_optional(pool).await.map_err(|e| e.to_string())?;
    // IBKR's per-day equity total does not itself carry a currency code;
    // the report's base currency is inferred from a flow's currency when
    // one exists, defaulting to USD otherwise (see module doc comment).
    let currency = currency_row.map(|(c,)| c).unwrap_or_else(|| "USD".to_string());

    let index = build_index(&equity, &flows);
    let daily_returns: Vec<Decimal> = index.windows(2).map(|w| w[1].index / w[0].index - Decimal::ONE).collect();
    let twr_index: Vec<metrics::IndexPoint> = index.iter().map(|p| metrics::IndexPoint { time_ms: p.time_ms, index: p.index }).collect();

    let sharpe = metrics::sharpe(&daily_returns, Decimal::ZERO);
    let sortino = metrics::sortino(&daily_returns, Decimal::ZERO);
    let max_drawdown = metrics::max_drawdown(&twr_index);
    let cagr = match (index.first(), index.last()) {
        (Some(first), Some(last)) if last.time_ms > first.time_ms => {
            let days_elapsed = ((last.time_ms - first.time_ms) / DAY_MS as u64) as u32;
            metrics::compute_cagr(first.index, last.index, days_elapsed)
        }
        _ => Err(metrics::CagrError::InsufficientSample { actual: 0, required: 365 }),
    };

    let fills: Vec<Fill> = trade_rows.iter().map(|(date, is_buy, qty, price)| Fill { time_ms: ms_at_date(*date) as u64, qty: *qty, price: *price, is_buy: *is_buy }).collect();
    let win_rate = metrics::compute_win_rate(&fills).ok();

    let net_flows: Decimal = flows.iter().map(|(_, amount)| *amount).sum();
    let total_change = match (equity.first(), equity.last()) {
        (Some((_, first)), Some((_, last))) => *last - *first - net_flows,
        _ => Decimal::ZERO,
    };

    Ok(IbkrPerformance {
        daily_nav: index.iter().map(|p| (p.time_ms, p.nav)).collect(),
        daily_returns,
        sharpe,
        sortino,
        max_drawdown,
        cagr,
        win_rate,
        earliest_reliable_checkpoint_ms: index.first().map(|p| p.time_ms),
        total_change,
        currency,
    })
}

pub async fn current_nav(pool: &PgPool, connection_id: Uuid) -> Result<Option<(Decimal, String)>, String> {
    let equity = stored_equity(pool, connection_id).await.map_err(|e| e.to_string())?;
    let currency_row: Option<(String,)> = sqlx::query_as("SELECT currency FROM ibkr_flows WHERE connection_id = $1 ORDER BY report_date LIMIT 1").bind(connection_id).fetch_optional(pool).await.map_err(|e| e.to_string())?;
    let currency = currency_row.map(|(c,)| c).unwrap_or_else(|| "USD".to_string());
    Ok(equity.last().map(|(_, total)| (*total, currency)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn a_deposit_does_not_count_as_a_gain() {
        let equity = vec![(date(2026, 1, 1), dec!(10000)), (date(2026, 1, 2), dec!(10500))];
        let flows = vec![(date(2026, 1, 2), dec!(500))];
        let points = build_index(&equity, &flows);
        assert_eq!(points[0].index, Decimal::ONE);
        // (10500 - 500) / 10000 - 1 = 0: flat, despite the balance rising.
        assert_eq!(points[1].index, Decimal::ONE);
    }

    #[test]
    fn a_real_gain_moves_the_index() {
        let equity = vec![(date(2026, 1, 1), dec!(10000)), (date(2026, 1, 2), dec!(11000))];
        let points = build_index(&equity, &[]);
        assert_eq!(points[1].index, dec!(1.1));
    }

    #[test]
    fn an_empty_series_builds_no_points() {
        assert!(build_index(&[], &[]).is_empty());
    }

    #[test]
    fn a_single_point_is_just_the_baseline() {
        let points = build_index(&[(date(2026, 1, 1), dec!(500))], &[]);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].index, Decimal::ONE);
        assert_eq!(points[0].nav, dec!(500));
    }
}
