//! Computes a real NAV snapshot for a connected account from its
//! current, real balance and real price marks — the first link between
//! collected raw data and `crates/domain/valuation`'s already-tested
//! NAV computation.
//!
//! Deliberately does **not** use `binance_worker::db`'s stored trades/
//! flows for this snapshot: `GET /api/v3/account` already gives the
//! authoritative current balance directly, and `valuation::value_at_cut`
//! is designed exactly for "the balance endpoint isn't perfectly atomic
//! across assets" — feeding it a snapshot with no surrounding delta
//! events is the correct degenerate case when the cut instant *is* the
//! snapshot instant (see `reconstruct_quantity`'s doc comment). Ledger
//! reconciliation from the collected trade/flow history is a genuinely
//! separate piece — historical NAV series (for returns/TWR/metrics) —
//! not needed for "what is this account worth right now."

use exchange_core::{parallel_map, AccountBalance, MarketData, MarketError};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use valuation::{value_at_cut, AssetInput, BalanceSnapshot, Valuation, ValuationError};

#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error(transparent)]
    Market(#[from] MarketError),
    #[error("balance amount for {asset} is not a valid decimal: {amount}")]
    InvalidBalanceAmount { asset: String, amount: String },
    #[error(transparent)]
    Valuation(#[from] ValuationError),
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the epoch")
        .as_millis() as u64
}

/// Resolves `asset`'s price in the exchange's quote currency at
/// `evaluated_at_ms` — "now" for [`compute_current_nav`], or a past
/// instant for [`crate::history`]'s daily reconstruction. The quote
/// currency itself is exactly `1` by definition (it *is* the
/// consolidated currency — a definitional identity, not an authenticated
/// mark). Every other asset needs a real, recently-closed (relative to
/// `evaluated_at_ms`) market price; an asset with no such market (e.g. a
/// fiat balance like BRL) has no price support and is reported as such,
/// not silently defaulted to any value.
pub fn resolve_historical_price(
    client: &dyn MarketData,
    asset: &str,
    evaluated_at_ms: u64,
) -> Result<Decimal, PublishError> {
    if asset == client.quote_currency() {
        return Ok(Decimal::ONE);
    }
    client
        .price_at_instant(asset, evaluated_at_ms)
        .map_err(|e| PublishError::Valuation(ValuationError::UnsupportedAsset(format!("{asset}: {e}"))))
}

/// Resolves many `(asset, instant)` prices concurrently, returning them
/// in the same order as `requests` — each is an independent public
/// price round trip (~0.3s), so doing them one after another is what
/// made a long history minutes-slow. Prices for past instants never
/// change, and current ones are fetched live every time: nothing here is
/// cached, only overlapped.
pub fn resolve_prices_parallel(
    client: &dyn MarketData,
    requests: &[(String, u64)],
) -> Vec<Result<Decimal, PublishError>> {
    parallel_map(requests, |(asset, at_ms)| {
        resolve_historical_price(client, asset, *at_ms)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavResult {
    pub valuation: Valuation,
    /// Held assets with no market against the quote currency at all — out of scope
    /// for the spot v0.1 profile (e.g. a fiat balance like BRL) — and
    /// therefore excluded from `valuation`, not silently priced. This
    /// is a different, and narrower, exclusion than "no *current*
    /// price available": an asset that *has* a market but no
    /// recently-closed candle still refuses the whole NAV, per
    /// `value_at_cut`'s own documented behavior.
    pub excluded_out_of_scope_assets: Vec<String>,
}

/// The real NAV computation: fetches the account's current balance,
/// resolves a real price for every nonzero, in-scope asset, and calls
/// `valuation::value_at_cut`. Still refuses the *entire* NAV if a
/// held, in-scope asset (one with a real market) has no price
/// currently available — only assets with no market at all are
/// excluded rather than causing a refusal.
pub fn compute_current_nav(client: &dyn MarketData) -> Result<NavResult, PublishError> {
    let quote = client.quote_currency().to_string();
    let balances: Vec<AccountBalance> = client.fetch_account_balances()?;
    let cut_ms = now_ms();

    let held: Vec<&AccountBalance> = balances
        .iter()
        .filter(|b| {
            let free = Decimal::from_str(&b.free).unwrap_or(Decimal::ZERO);
            let locked = Decimal::from_str(&b.locked).unwrap_or(Decimal::ZERO);
            free + locked > Decimal::ZERO
        })
        .collect();

    // Only the pairs actually needed are looked up (a handful), not the
    // whole ~17 MB exchange catalog.
    let wanted_markets: std::collections::BTreeSet<String> = held
        .iter()
        .filter(|b| b.asset != quote)
        .map(|b| client.market_symbol(&b.asset))
        .collect();
    let listed_markets = client.fetch_symbol_assets_for(&wanted_markets)?;

    let mut excluded_out_of_scope_assets = Vec::new();
    let mut in_scope = Vec::with_capacity(held.len());
    for balance in held {
        if balance.asset == quote || listed_markets.contains_key(&client.market_symbol(&balance.asset)) {
            in_scope.push(balance);
        } else {
            excluded_out_of_scope_assets.push(balance.asset.clone());
        }
    }

    let resolved_prices = resolve_prices_parallel(
        client,
        &in_scope.iter().map(|b| (b.asset.clone(), cut_ms)).collect::<Vec<_>>(),
    );
    let mut resolved_prices = resolved_prices.into_iter();

    let mut prices = BTreeMap::new();
    let mut inputs = Vec::with_capacity(in_scope.len());
    for balance in &in_scope {
        let free =
            Decimal::from_str(&balance.free).map_err(|_| PublishError::InvalidBalanceAmount {
                asset: balance.asset.clone(),
                amount: balance.free.clone(),
            })?;
        let locked =
            Decimal::from_str(&balance.locked).map_err(|_| PublishError::InvalidBalanceAmount {
                asset: balance.asset.clone(),
                amount: balance.locked.clone(),
            })?;
        let price = resolved_prices
            .next()
            .expect("one resolved price per in-scope asset")?;
        prices.insert(balance.asset.clone(), price);
        inputs.push(AssetInput {
            asset: balance.asset.clone(),
            snapshot: BalanceSnapshot {
                free,
                locked,
                observed_at_ms: cut_ms,
            },
            events: &[],
        });
    }

    let valuation = value_at_cut(&inputs, &prices, cut_ms)?;
    Ok(NavResult {
        valuation,
        excluded_out_of_scope_assets,
    })
}
