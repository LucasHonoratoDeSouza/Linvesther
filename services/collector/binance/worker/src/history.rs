//! Historical NAV series and time-weighted returns, from the trades/
//! flows this connection has actually collected — the piece that turns
//! `publish.rs`'s single "NAV right now" into a real return series
//! `crates/domain/returns`/`metrics` can compute from.
//!
//! **The reconstruction anchor is the real, current balance** (`GET
//! /api/v3/account`), not an assumption that collected history is
//! complete from account inception. `valuation::reconstruct_quantity`
//! is built exactly for this: given a snapshot at one instant and the
//! ledger deltas around it, it can walk *backward* in time (undoing
//! events between the cut and the snapshot) as validly as forward. A
//! historical NAV point before this connection existed is therefore
//! only as complete as the collected trade/flow history is — if a
//! trade happened before the account was connected, that day's
//! reconstructed quantity is wrong by that trade's effect. This is a
//! real, documented limitation, not hidden: `HistoryResult` reports
//! `earliest_reliable_checkpoint_ms` (the first collected event's own
//! time) so a caller never treats an earlier point as trustworthy.

use crate::db;
use binance_flows::NormalizedFlow;
use binance_trades::Trade;
use ledger::{LedgerEvent, LedgerLeg};
use returns::{compute_twr, ReturnError, TimelineEvent};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::str::FromStr;
use uuid::Uuid;
use valuation::{reconstruct_quantity, BalanceSnapshot, TimedDelta, ValuationError};

use crate::pnl::{compute_pnl, PnlEvent, PnlSummary};
use crate::publish::resolve_prices_parallel;
use exchange_core::{parallel_map, MarketData, MarketError, RawCandle, SERIES_STEPS_MS};
use std::sync::Arc;

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Market(#[from] MarketError),
    #[error("trade {0} references symbol {1}, which has no known base/quote asset split")]
    UnknownSymbol(u64, String),
    #[error("trade or flow amount is not a valid decimal: {0}")]
    InvalidAmount(String),
    #[error(transparent)]
    Valuation(#[from] ValuationError),
    #[error(transparent)]
    Return(#[from] ReturnError),
    #[error(transparent)]
    Publish(#[from] crate::publish::PublishError),
    #[error("background task panicked: {0}")]
    Join(String),
}

/// One daily checkpoint's real, reconstructed NAV.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyNav {
    pub day_start_ms: u64,
    pub nav: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryResult {
    pub daily_nav: Vec<DailyNav>,
    /// Per-day TWR subperiod returns, in the same order — this is
    /// exactly the "daily returns" series `metrics::stats` (Sharpe/
    /// Sortino) consume.
    pub daily_returns: Vec<Decimal>,
    /// The TWR index, chained from `1` at the first checkpoint —
    /// unlike `daily_nav`, this excludes the effect of deposits/
    /// withdrawals entirely, so it (not raw NAV) is what
    /// `metrics::compute_cagr`/`max_drawdown` should consume; feeding
    /// them raw NAV would overstate return by counting deposits as
    /// gains — the exact bug this module's own development caught and
    /// fixed once already (see `daily_returns`' doc comment history).
    /// Reuses `metrics::IndexPoint` directly rather than a redundant
    /// local type, since that's exactly what `max_drawdown` consumes.
    pub twr_index: Vec<metrics::IndexPoint>,
    /// The instant the account was connected — everything here is
    /// measured from then on, and nothing before it is claimed (see
    /// module doc comment).
    pub earliest_reliable_checkpoint_ms: Option<u64>,
    /// FIFO win rate across every symbol's closed round trips combined
    /// (see `metrics::winrate`'s own doc comment for what a "round
    /// trip" means here) — `None` when there are no closed round trips
    /// yet, never a fabricated `0%`.
    pub win_rate: Option<metrics::WinRateResult>,
    /// Realized/unrealized profit per position since the connection.
    pub pnl: PnlSummary,
}

/// FIFO win rate is computed per symbol (mixing lots across different
/// assets would match a BTC buy against an ETH sell, which is not a
/// real round trip), then combined into one overall rate across every
/// symbol's closed round trips.
fn compute_overall_win_rate(
    trades: &[Trade],
) -> Result<Option<metrics::WinRateResult>, HistoryError> {
    let mut by_symbol: BTreeMap<String, Vec<metrics::Fill>> = BTreeMap::new();
    for trade in trades {
        let price = parse_decimal(&trade.price)?;
        let qty = parse_decimal(&trade.qty)?;
        by_symbol
            .entry(trade.symbol.clone())
            .or_default()
            .push(metrics::Fill {
                time_ms: trade.time_ms,
                qty,
                price,
                is_buy: trade.is_buyer,
            });
    }

    let mut total_closed = 0usize;
    let mut total_wins = 0usize;
    for fills in by_symbol.values() {
        match metrics::compute_win_rate(fills) {
            Ok(result) => {
                total_closed += result.closed_round_trips;
                total_wins += result.wins;
            }
            Err(metrics::WinRateError::NoClosedRoundTrips) => {}
        }
    }

    if total_closed == 0 {
        return Ok(None);
    }
    Ok(Some(metrics::WinRateResult {
        closed_round_trips: total_closed,
        wins: total_wins,
        win_rate: Decimal::from(total_wins as u64) / Decimal::from(total_closed as u64),
    }))
}

fn parse_decimal(s: &str) -> Result<Decimal, HistoryError> {
    Decimal::from_str(s).map_err(|_| HistoryError::InvalidAmount(s.to_string()))
}

fn trade_to_ledger_event(
    trade: &Trade,
    base: &str,
    quote: &str,
) -> Result<LedgerEvent, HistoryError> {
    let price = parse_decimal(&trade.price)?;
    let qty = parse_decimal(&trade.qty)?;
    let quote_amount = (price * qty).to_string();
    let (base_leg, quote_leg) = if trade.is_buyer {
        (
            LedgerLeg {
                asset: base.to_string(),
                amount: trade.qty.clone(),
                is_credit: true,
            },
            LedgerLeg {
                asset: quote.to_string(),
                amount: quote_amount,
                is_credit: false,
            },
        )
    } else {
        (
            LedgerLeg {
                asset: base.to_string(),
                amount: trade.qty.clone(),
                is_credit: false,
            },
            LedgerLeg {
                asset: quote.to_string(),
                amount: quote_amount,
                is_credit: true,
            },
        )
    };
    Ok(LedgerEvent {
        source_namespace: "binance.trade".to_string(),
        source_id: format!("{}#{}", trade.symbol, trade.id),
        economic_time_ms: trade.time_ms,
        observed_time_ms: trade.time_ms,
        legs: vec![base_leg, quote_leg],
        fee: Some(LedgerLeg {
            asset: trade.commission_asset.clone(),
            amount: trade.commission.clone(),
            is_credit: false,
        }),
    })
}

fn flow_to_ledger_event(flow: &NormalizedFlow) -> LedgerEvent {
    LedgerEvent {
        source_namespace: flow.source_namespace.to_string(),
        source_id: flow.source_id.clone(),
        economic_time_ms: flow.economic_time_ms,
        observed_time_ms: flow.economic_time_ms,
        legs: flow
            .legs
            .iter()
            .map(|l| LedgerLeg {
                asset: l.asset.clone(),
                amount: l.amount.clone(),
                is_credit: l.is_credit,
            })
            .collect(),
        fee: flow.fee.as_ref().map(|l| LedgerLeg {
            asset: l.asset.clone(),
            amount: l.amount.clone(),
            is_credit: l.is_credit,
        }),
    }
}

/// Per-asset signed `TimedDelta`s from every collected ledger event —
/// what `reconstruct_quantity` needs to walk from the current real
/// balance back to any earlier instant.
fn per_asset_deltas(
    events: &[LedgerEvent],
) -> Result<BTreeMap<String, Vec<TimedDelta>>, HistoryError> {
    let mut deltas: BTreeMap<String, Vec<TimedDelta>> = BTreeMap::new();
    for event in events {
        for leg in event.legs.iter().chain(event.fee.iter()) {
            let amount = parse_decimal(&leg.amount)?;
            let signed = if leg.is_credit { amount } else { -amount };
            deltas
                .entry(leg.asset.clone())
                .or_default()
                .push(TimedDelta {
                    time_ms: event.economic_time_ms,
                    delta: signed,
                });
        }
    }
    Ok(deltas)
}

/// Builds the full ledger event list from everything this connection
/// has collected. Fails closed (`UnknownSymbol`) on a trade whose
/// symbol has no known base/quote split, rather than silently
/// dropping it from the reconstruction.
fn collect_ledger_events(
    trades: &[Trade],
    flows: &[NormalizedFlow],
    symbol_assets: &BTreeMap<String, (String, String)>,
) -> Result<Vec<LedgerEvent>, HistoryError> {
    let mut events = Vec::with_capacity(trades.len() + flows.len());
    for trade in trades {
        let (base, quote) = symbol_assets
            .get(&trade.symbol)
            .ok_or_else(|| HistoryError::UnknownSymbol(trade.id, trade.symbol.clone()))?;
        events.push(trade_to_ledger_event(trade, base, quote)?);
    }
    for flow in flows {
        events.push(flow_to_ledger_event(flow));
    }
    events.sort_by_key(|e| e.economic_time_ms);
    Ok(events)
}

/// Reconstructs each held asset's quantity at `cut_ms`, given the real
/// current balance as the anchor snapshot and the full collected
/// ledger's per-asset deltas.
fn quantities_at(
    cut_ms: u64,
    now_ms: u64,
    current_balances: &BTreeMap<String, (Decimal, Decimal)>,
    deltas: &BTreeMap<String, Vec<TimedDelta>>,
) -> Result<BTreeMap<String, Decimal>, HistoryError> {
    let mut quantities = BTreeMap::new();
    for (asset, (free, locked)) in current_balances {
        let snapshot = BalanceSnapshot {
            free: *free,
            locked: *locked,
            observed_at_ms: now_ms,
        };
        let empty = Vec::new();
        let events = deltas.get(asset).unwrap_or(&empty);
        let quantity = reconstruct_quantity(&snapshot, events, cut_ms).map_err(|source| {
            ValuationError::AmbiguousOrdering {
                asset: asset.clone(),
                source,
            }
        })?;
        quantities.insert(asset.clone(), quantity);
    }
    Ok(quantities)
}

/// What every reconstruction starts from: only what happened since the
/// connection, the ledger's per-asset deltas, and the account's real,
/// current balances (the anchor the deltas are walked back from).
struct Prepared {
    trades: Vec<Trade>,
    flows: Vec<NormalizedFlow>,
    symbol_assets: BTreeMap<String, (String, String)>,
    deltas: BTreeMap<String, Vec<TimedDelta>>,
    current_balances: BTreeMap<String, (Decimal, Decimal)>,
}

fn prepare(
    all_trades: &[Trade],
    all_flows: &[NormalizedFlow],
    client: &dyn MarketData,
    since_ms: u64,
) -> Result<Prepared, HistoryError> {
    let pool_trades: Vec<Trade> = all_trades
        .iter()
        .filter(|t| t.time_ms >= since_ms)
        .cloned()
        .collect();
    let pool_flows: Vec<NormalizedFlow> = all_flows
        .iter()
        .filter(|f| f.economic_time_ms >= since_ms)
        .cloned()
        .collect();
    let pool_trades_slice = pool_trades.as_slice();
    let pool_flows_slice = pool_flows.as_slice();

    // Only the symbols this account's own trades used, then (below) the
    // `{asset}USDT` markets of the assets it actually holds or touched —
    // not the whole ~17 MB exchange catalog on every call.
    let trade_symbols: std::collections::BTreeSet<String> =
        pool_trades_slice.iter().map(|t| t.symbol.clone()).collect();
    let quote = client.quote_currency().to_string();
    let mut symbol_assets = client.fetch_symbol_assets_for(&trade_symbols)?;
    let ledger_events = collect_ledger_events(pool_trades_slice, pool_flows_slice, &symbol_assets)?;
    let deltas = per_asset_deltas(&ledger_events)?;

    let balances = client.fetch_account_balances()?;
    let wanted_markets: std::collections::BTreeSet<String> = balances
        .iter()
        .filter(|b| b.asset != quote)
        .filter(|b| {
            let free = Decimal::from_str(&b.free).unwrap_or(Decimal::ZERO);
            let locked = Decimal::from_str(&b.locked).unwrap_or(Decimal::ZERO);
            free + locked > Decimal::ZERO || deltas.contains_key(&b.asset)
        })
        .map(|b| client.market_symbol(&b.asset))
        .filter(|market| !symbol_assets.contains_key(market))
        .collect();
    symbol_assets.extend(client.fetch_symbol_assets_for(&wanted_markets)?);
    let mut current_balances = BTreeMap::new();
    for b in &balances {
        let free = parse_decimal(&b.free)?;
        let locked = parse_decimal(&b.locked)?;
        let held_or_touched = free + locked > Decimal::ZERO || deltas.contains_key(&b.asset);
        // Same out-of-scope exclusion as publish.rs::compute_current_nav:
        // an asset with no {asset}USDT market at all (e.g. a fiat
        // balance like BRL) is outside the spot v0.1 profile, excluded
        // from the reconstruction entirely rather than causing every
        // checkpoint to refuse.
        let in_scope = b.asset == quote || symbol_assets.contains_key(&client.market_symbol(&b.asset));
        if held_or_touched && in_scope {
            current_balances.insert(b.asset.clone(), (free, locked));
        }
    }

    Ok(Prepared {
        trades: pool_trades,
        flows: pool_flows,
        symbol_assets,
        deltas,
        current_balances,
    })
}

/// Computes the NAV series, TWR returns and PnL for one connection,
/// **from the instant it was connected (`since_ms`) onward**: what the
/// account held then is the opening balance, and only trades and
/// deposits/withdrawals from then on move the numbers. The real,
/// current balance is still the reconstruction anchor, walked backwards
/// through the deltas collected since `since_ms`.
pub fn compute_history(
    all_trades: &[Trade],
    all_flows: &[NormalizedFlow],
    client: &dyn MarketData,
    since_ms: u64,
    now_ms: u64,
) -> Result<HistoryResult, HistoryError> {
    let Prepared {
        trades: pool_trades,
        flows: pool_flows,
        symbol_assets,
        deltas,
        current_balances,
    } = prepare(all_trades, all_flows, client, since_ms)?;
    let pool_trades = pool_trades.as_slice();
    let pool_flows = pool_flows.as_slice();
    let quote = client.quote_currency().to_string();

    // The connection instant itself (the opening balance), then every
    // UTC day boundary after it, then now — a real calendar cadence,
    // never reaching back before the account was connected.
    let mut checkpoints = vec![since_ms];
    let mut t = (since_ms / DAY_MS + 1) * DAY_MS;
    while t < now_ms {
        checkpoints.push(t);
        t += DAY_MS;
    }
    if *checkpoints.last().unwrap_or(&0) != now_ms && now_ms > since_ms {
        checkpoints.push(now_ms);
    }

    // External capital (deposits/withdrawals) must enter the TWR
    // timeline as `Flow` events, not folded into a `Valuation` jump —
    // ("aporte de USD 100.000 sem lucro em carteira de USD
    // 10.000 DEVE produzir retorno zero"), a deposit must never be
    // read back as a day's investment return. Trades are excluded here
    // (an internal asset swap, not external capital); Convert/Dust/
    // Dividend/Transfer are not yet classified as external-vs-internal
    // and are excluded too — a real, documented gap, not silently
    // treated as neither.
    let external_flows: Vec<&NormalizedFlow> = pool_flows
        .iter()
        .filter(|flow| {
            matches!(
                flow.source_namespace,
                binance_flows::FlowFamily::Deposit | binance_flows::FlowFamily::Withdrawal
            )
        })
        .collect();
    let flow_price_requests: Vec<(String, u64)> = external_flows
        .iter()
        .flat_map(|flow| {
            flow.legs
                .iter()
                .chain(flow.fee.iter())
                .map(|leg| (leg.asset.clone(), flow.economic_time_ms))
        })
        .collect();
    let mut flow_prices = resolve_prices_parallel(client, &flow_price_requests).into_iter();

    let mut flow_events: Vec<(u64, Decimal)> = Vec::new();
    let mut pnl_events: Vec<(u64, PnlEvent)> = Vec::new();
    for flow in external_flows {
        let mut net_amount = Decimal::ZERO;
        let principal_legs = flow.legs.len();
        for (index, leg) in flow.legs.iter().chain(flow.fee.iter()).enumerate() {
            let amount = parse_decimal(&leg.amount)?;
            let price = flow_prices
                .next()
                .expect("one resolved price per flow leg")?;
            let value = amount * price;
            net_amount += if leg.is_credit { value } else { -value };
            if leg.asset != quote {
                let event = if leg.is_credit {
                    PnlEvent::Acquire {
                        asset: leg.asset.clone(),
                        quantity: amount,
                        unit_price: price,
                    }
                } else {
                    PnlEvent::Withdraw {
                        asset: leg.asset.clone(),
                        quantity: amount,
                    }
                };
                pnl_events.push((flow.economic_time_ms, event));
            }
            if index >= principal_legs {
                pnl_events.push((flow.economic_time_ms, PnlEvent::Fee { value }));
            }
        }
        flow_events.push((flow.economic_time_ms, net_amount));
    }

    let mut daily_nav = Vec::with_capacity(checkpoints.len());
    let mut timeline: Vec<(u64, TimelineEvent)> =
        Vec::with_capacity(checkpoints.len() + flow_events.len());
    // Quantities at every checkpoint are pure arithmetic; only the
    // prices need the network, so resolve all of those concurrently up
    // front, then walk the checkpoints in order exactly as before.
    let mut checkpoint_quantities = Vec::with_capacity(checkpoints.len());
    let mut nav_price_requests: Vec<(String, u64)> = Vec::new();
    for &cut_ms in &checkpoints {
        let quantities = quantities_at(cut_ms, now_ms, &current_balances, &deltas)?;
        for (asset, qty) in &quantities {
            if *qty != Decimal::ZERO {
                nav_price_requests.push((asset.clone(), cut_ms));
            }
        }
        checkpoint_quantities.push(quantities);
    }
    let mut nav_prices = resolve_prices_parallel(client, &nav_price_requests).into_iter();

    let last_checkpoint_index = checkpoints.len() - 1;
    let mut opening_positions: Vec<(String, Decimal, Decimal)> = Vec::new();
    let mut current_marks: BTreeMap<String, Decimal> = BTreeMap::new();
    for (index, (&cut_ms, quantities)) in checkpoints
        .iter()
        .zip(&checkpoint_quantities)
        .enumerate()
    {
        let mut nav = Decimal::ZERO;
        for (asset, qty) in quantities {
            if *qty == Decimal::ZERO {
                continue;
            }
            let price = nav_prices
                .next()
                .expect("one resolved price per nonzero holding")?;
            nav += qty * price;
            if index == 0 && *asset != quote {
                opening_positions.push((asset.clone(), *qty, price));
            }
            if index == last_checkpoint_index {
                current_marks.insert(asset.clone(), price);
            }
        }
        daily_nav.push(DailyNav {
            day_start_ms: cut_ms,
            nav,
        });
        timeline.push((
            cut_ms,
            TimelineEvent::Valuation {
                time_ms: cut_ms,
                nav,
            },
        ));
    }
    for (time_ms, amount) in flow_events {
        timeline.push((time_ms, TimelineEvent::Flow { time_ms, amount }));
    }
    // Chronological, and a Flow at the same instant as a Valuation
    // sorts before it — the flow's effect belongs to the checkpoint
    // that closes *after* it, never the one it's simultaneous with.
    timeline.sort_by_key(|(t, event)| (*t, matches!(event, TimelineEvent::Valuation { .. })));
    let ordered_events: Vec<TimelineEvent> = timeline.into_iter().map(|(_, event)| event).collect();

    let twr = compute_twr(&ordered_events)?;

    // Every Valuation after the first closes one subperiod, so
    // sub_returns lines up 1:1 with daily_nav[1..] — chain them into a
    // real index series (SCALE=1 at the first checkpoint).
    let mut twr_index = Vec::with_capacity(twr.sub_returns.len() + 1);
    if let Some(first) = daily_nav.first() {
        let mut index = Decimal::ONE;
        twr_index.push(metrics::IndexPoint {
            time_ms: first.day_start_ms,
            index,
        });
        for (point, r) in daily_nav.iter().skip(1).zip(&twr.sub_returns) {
            index *= Decimal::ONE + r;
            twr_index.push(metrics::IndexPoint {
                time_ms: point.day_start_ms,
                index,
            });
        }
    }

    let win_rate = compute_overall_win_rate(pool_trades)?;

    // Every fill since the connection becomes two priced legs (the base
    // asset and the quote asset it was traded against), valued in USDT
    // at the instant of the trade, plus its fee.
    let mut trade_price_requests: Vec<(String, u64)> = Vec::new();
    for trade in pool_trades {
        let (_, quote_asset) = symbol_assets
            .get(&trade.symbol)
            .ok_or_else(|| HistoryError::UnknownSymbol(trade.id, trade.symbol.clone()))?;
        trade_price_requests.push((quote_asset.clone(), trade.time_ms));
        trade_price_requests.push((trade.commission_asset.clone(), trade.time_ms));
    }
    let mut trade_prices = resolve_prices_parallel(client, &trade_price_requests).into_iter();
    for trade in pool_trades {
        let (base, quote_asset) = &symbol_assets[&trade.symbol];
        let quote_usdt = trade_prices.next().expect("one quote price per trade")?;
        let fee_asset_usdt = trade_prices.next().expect("one fee price per trade")?;
        let price = parse_decimal(&trade.price)?;
        let quantity = parse_decimal(&trade.qty)?;
        let base_usdt = price * quote_usdt;
        let quote_quantity = quantity * price;
        let (base_event, quote_event) = if trade.is_buyer {
            (
                PnlEvent::Acquire { asset: base.clone(), quantity, unit_price: base_usdt },
                PnlEvent::Dispose { asset: quote_asset.clone(), quantity: quote_quantity, unit_price: quote_usdt },
            )
        } else {
            (
                PnlEvent::Dispose { asset: base.clone(), quantity, unit_price: base_usdt },
                PnlEvent::Acquire { asset: quote_asset.clone(), quantity: quote_quantity, unit_price: quote_usdt },
            )
        };
        if *base != quote {
            pnl_events.push((trade.time_ms, base_event));
        }
        if *quote_asset != quote {
            pnl_events.push((trade.time_ms, quote_event));
        }
        let fee = parse_decimal(&trade.commission)? * fee_asset_usdt;
        pnl_events.push((trade.time_ms, PnlEvent::Fee { value: fee }));
    }
    pnl_events.sort_by_key(|(time_ms, _)| *time_ms);
    let pnl_events: Vec<PnlEvent> = pnl_events.into_iter().map(|(_, event)| event).collect();

    let current_quantities: BTreeMap<String, Decimal> = checkpoint_quantities
        .last()
        .map(|quantities| {
            quantities
                .iter()
                .filter(|(asset, _)| **asset != quote)
                .map(|(asset, qty)| (asset.clone(), *qty))
                .collect()
        })
        .unwrap_or_default();
    let pnl = compute_pnl(&opening_positions, &pnl_events, &current_quantities, &current_marks);

    Ok(HistoryResult {
        daily_nav,
        daily_returns: twr.sub_returns,
        twr_index,
        earliest_reliable_checkpoint_ms: Some(since_ms),
        win_rate,
        pnl,
    })
}

/// One point of the account's value over time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesPoint {
    pub time_ms: u64,
    /// What the account was worth then, in USDT (deposits and
    /// withdrawals move this).
    pub nav: Decimal,
    /// The time-weighted return index at that instant, `1` at the first
    /// point — deposits and withdrawals never move it, so it (not `nav`)
    /// is the honest picture of performance.
    pub index: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesResult {
    pub points: Vec<SeriesPoint>,
    pub step_ms: u64,
}

/// A chart range and the resolution it is drawn at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesRange {
    Day,
    Week,
    Month,
    Year,
    FiveYears,
    /// Everything since the account was connected, at whatever
    /// resolution keeps it readable.
    Max,
}

impl SeriesRange {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "24h" => Self::Day,
            "7d" => Self::Week,
            "30d" => Self::Month,
            "1y" => Self::Year,
            "5y" => Self::FiveYears,
            "max" => Self::Max,
            _ => return None,
        })
    }

    pub fn window_ms(self) -> Option<u64> {
        match self {
            Self::Day => Some(DAY_MS),
            Self::Week => Some(7 * DAY_MS),
            Self::Month => Some(30 * DAY_MS),
            Self::Year => Some(365 * DAY_MS),
            Self::FiveYears => Some(5 * 365 * DAY_MS),
            Self::Max => None,
        }
    }
}

/// Roughly 300–400 points per range keeps a chart readable and cheap.
/// Every step is a candle size all exchanges offer (see
/// [`SERIES_STEPS_MS`]).
fn step_for(range: SeriesRange, span_ms: u64) -> u64 {
    match range {
        SeriesRange::Day => 300_000,
        SeriesRange::Week => 1_800_000,
        SeriesRange::Month => 7_200_000,
        SeriesRange::Year | SeriesRange::FiveYears => DAY_MS,
        SeriesRange::Max => SERIES_STEPS_MS
            .iter()
            .copied()
            .find(|step| span_ms / step <= 400)
            .unwrap_or(DAY_MS),
    }
}

/// The last fully closed candle's close at or before `at_ms` — what the
/// market was at that point, never a later price. `now_ms` itself takes
/// the latest candle (the live price), since nothing is "later" than it.
fn price_at(candles: &[RawCandle], at_ms: u64, is_now: bool) -> Option<Decimal> {
    let candle = if is_now {
        candles.last()
    } else {
        candles.iter().rev().find(|c| c.close_time_ms <= at_ms)
    }?;
    Decimal::from_str(&candle.close_price).ok()
}

/// The account's value and time-weighted return over `range`, drawn from
/// real market candles — one bulk request per held asset rather than one
/// per point — and never reaching back before the connection.
pub fn compute_series(
    all_trades: &[Trade],
    all_flows: &[NormalizedFlow],
    client: &dyn MarketData,
    since_ms: u64,
    now_ms: u64,
    range: SeriesRange,
) -> Result<SeriesResult, HistoryError> {
    let Prepared {
        flows: pool_flows,
        deltas,
        current_balances,
        ..
    } = prepare(all_trades, all_flows, client, since_ms)?;
    let quote = client.quote_currency().to_string();

    let start_ms = range.window_ms().map_or(since_ms, |w| now_ms.saturating_sub(w).max(since_ms));
    let step_ms = step_for(range, now_ms.saturating_sub(start_ms));

    // Interior points sit exactly on candle boundaries (multiples of the
    // step), where "the close of the candle that just ended" *is* the
    // market price at that instant. The two ends are not on a boundary:
    // the first (the moment of connection) gets its exact price from
    // the 1-minute mark, the last (now) the live price.
    let mut grid = vec![start_ms];
    let mut t = (start_ms / step_ms + 1) * step_ms;
    while t < now_ms {
        grid.push(t);
        t += step_ms;
    }
    if now_ms > start_ms {
        grid.push(now_ms);
    }

    let mut grid_quantities = Vec::with_capacity(grid.len());
    let mut assets: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for &at in &grid {
        let quantities = quantities_at(at, now_ms, &current_balances, &deltas)?;
        for (asset, qty) in &quantities {
            if *qty != Decimal::ZERO && *asset != quote {
                assets.insert(asset.clone());
            }
        }
        grid_quantities.push(quantities);
    }

    // One bulk candle request per held asset, starting a couple of steps
    // early so there is always a closed candle before the first point.
    let asset_list: Vec<String> = assets.into_iter().collect();
    let fetched = parallel_map(&asset_list, |asset| {
        client.fetch_candles_span(
            &client.market_symbol(asset),
            step_ms,
            start_ms.saturating_sub(2 * step_ms),
            now_ms,
        )
    });
    let mut candles: BTreeMap<String, Vec<RawCandle>> = BTreeMap::new();
    for (asset, result) in asset_list.iter().zip(fetched) {
        candles.insert(asset.clone(), result?);
    }

    let first_requests: Vec<(String, u64)> = grid_quantities[0]
        .iter()
        .filter(|(asset, qty)| **qty != Decimal::ZERO && asset.as_str() != quote)
        .map(|(asset, _)| (asset.clone(), grid[0]))
        .collect();
    let mut first_prices: BTreeMap<String, Decimal> = BTreeMap::new();
    for ((asset, _), price) in first_requests.iter().zip(resolve_prices_parallel(client, &first_requests)) {
        first_prices.insert(asset.clone(), price?);
    }

    let mut points = Vec::with_capacity(grid.len());
    let mut timeline: Vec<(u64, TimelineEvent)> = Vec::new();
    for (index, (&at, quantities)) in grid.iter().zip(&grid_quantities).enumerate() {
        let is_now = index == grid.len() - 1 && index > 0;
        let mut nav = Decimal::ZERO;
        for (asset, qty) in quantities {
            if *qty == Decimal::ZERO {
                continue;
            }
            let price = if *asset == quote {
                Decimal::ONE
            } else if index == 0 {
                first_prices[asset]
            } else {
                price_at(&candles[asset], at, is_now).ok_or_else(|| {
                    HistoryError::Valuation(ValuationError::UnsupportedAsset(format!("{asset}: no candle at {at}")))
                })?
            };
            nav += qty * price;
        }
        points.push((at, nav));
        timeline.push((at, TimelineEvent::Valuation { time_ms: at, nav }));
    }

    // Deposits/withdrawals since the window began enter the return as
    // flows, exactly as in `compute_history`, so they never read as gains.
    let window_flows: Vec<&NormalizedFlow> = pool_flows
        .iter()
        .filter(|flow| {
            flow.economic_time_ms >= start_ms
                && matches!(
                    flow.source_namespace,
                    binance_flows::FlowFamily::Deposit | binance_flows::FlowFamily::Withdrawal
                )
        })
        .collect();
    let requests: Vec<(String, u64)> = window_flows
        .iter()
        .flat_map(|flow| {
            flow.legs
                .iter()
                .chain(flow.fee.iter())
                .map(|leg| (leg.asset.clone(), flow.economic_time_ms))
        })
        .collect();
    let mut prices = resolve_prices_parallel(client, &requests).into_iter();
    for flow in window_flows {
        let mut net = Decimal::ZERO;
        for leg in flow.legs.iter().chain(flow.fee.iter()) {
            let value = parse_decimal(&leg.amount)? * prices.next().expect("one price per flow leg")?;
            net += if leg.is_credit { value } else { -value };
        }
        timeline.push((
            flow.economic_time_ms,
            TimelineEvent::Flow {
                time_ms: flow.economic_time_ms,
                amount: net,
            },
        ));
    }
    timeline.sort_by_key(|(t, event)| (*t, matches!(event, TimelineEvent::Valuation { .. })));
    let ordered: Vec<TimelineEvent> = timeline.into_iter().map(|(_, event)| event).collect();
    let twr = compute_twr(&ordered)?;

    let mut index = Decimal::ONE;
    let mut series = Vec::with_capacity(points.len());
    for (position, (at, nav)) in points.into_iter().enumerate() {
        if position > 0 {
            if let Some(r) = twr.sub_returns.get(position - 1) {
                index *= Decimal::ONE + r;
            }
        }
        series.push(SeriesPoint { time_ms: at, nav, index });
    }
    Ok(SeriesResult { points: series, step_ms })
}

/// Loads the connection's stored trades/flows and its connection instant,
/// then computes the series on a blocking task (see
/// [`load_and_compute_history`]).
pub async fn load_and_compute_series(
    pool: &PgPool,
    connection_id: Uuid,
    client: Arc<dyn MarketData>,
    now_ms: u64,
    range: SeriesRange,
) -> Result<SeriesResult, HistoryError> {
    let trades = db::fetch_stored_trades(pool, connection_id).await?;
    let flows = db::fetch_stored_flows(pool, connection_id).await?;
    let since_ms = db::connection_created_at_ms(pool, connection_id).await?;
    series_from(trades, flows, since_ms, client, now_ms, range).await
}

/// [`compute_series`] over already-loaded trades and flows, on a blocking task.
pub async fn series_from(
    trades: Vec<Trade>,
    flows: Vec<NormalizedFlow>,
    since_ms: u64,
    client: Arc<dyn MarketData>,
    now_ms: u64,
    range: SeriesRange,
) -> Result<SeriesResult, HistoryError> {
    tokio::task::spawn_blocking(move || compute_series(&trades, &flows, client.as_ref(), since_ms, now_ms, range))
        .await
        .map_err(|e| HistoryError::Join(e.to_string()))?
}

/// Loads the connection's stored trades/flows, then runs
/// [`compute_history`] on a blocking task — `compute_history` makes
/// blocking HTTP calls (klines/balances/symbol info) via `client`, and
/// running those directly in this async fn would nest a second tokio
/// runtime inside the first and panic on drop (`LiveClient`'s own doc
/// comment). `LiveClient` is cheap to clone (`Arc`-backed internally)
/// specifically so it can move into the blocking closure.
pub async fn load_and_compute_history(
    pool: &PgPool,
    connection_id: Uuid,
    client: Arc<dyn MarketData>,
    now_ms: u64,
) -> Result<HistoryResult, HistoryError> {
    let trades = db::fetch_stored_trades(pool, connection_id).await?;
    let flows = db::fetch_stored_flows(pool, connection_id).await?;
    let since_ms = db::connection_created_at_ms(pool, connection_id).await?;
    history_from(trades, flows, since_ms, client, now_ms).await
}

/// [`compute_history`] over already-loaded trades and flows, on a blocking task.
pub async fn history_from(
    trades: Vec<Trade>,
    flows: Vec<NormalizedFlow>,
    since_ms: u64,
    client: Arc<dyn MarketData>,
    now_ms: u64,
) -> Result<HistoryResult, HistoryError> {
    tokio::task::spawn_blocking(move || compute_history(&trades, &flows, client.as_ref(), since_ms, now_ms))
        .await
        .map_err(|e| HistoryError::Join(e.to_string()))?
}

/// Every metric a real performance dashboard needs, derived from one
/// [`HistoryResult`]. Each field is independently `None`/an explicit
/// refusal reason when its own minimum sample isn't met yet — never a
/// fabricated number standing in for "not enough history."
#[derive(Debug)]
pub struct PerformanceSummary {
    pub daily_nav: Vec<DailyNav>,
    pub daily_returns: Vec<Decimal>,
    pub sharpe: Result<Decimal, metrics::StatsError>,
    pub sortino: Result<Decimal, metrics::StatsError>,
    pub max_drawdown: Option<metrics::DrawdownEpisode>,
    pub cagr: Result<Decimal, metrics::CagrError>,
    pub win_rate: Option<metrics::WinRateResult>,
    pub earliest_reliable_checkpoint_ms: Option<u64>,
    pub pnl: PnlSummary,
}

/// Computes every metric from a [`HistoryResult`] — the same real
/// Sharpe/Sortino/MDD/CAGR wiring `tests/history_test.rs` proved
/// against a real account, factored out for `main.rs`'s `performance`
/// subcommand to reuse directly.
pub fn summarize_performance(history: HistoryResult) -> PerformanceSummary {
    let sharpe = metrics::sharpe(&history.daily_returns, Decimal::ZERO);
    let sortino = metrics::sortino(&history.daily_returns, Decimal::ZERO);
    let max_drawdown = metrics::max_drawdown(&history.twr_index);
    let cagr = match (history.twr_index.first(), history.twr_index.last()) {
        (Some(first), Some(last)) if last.time_ms > first.time_ms => {
            let days_elapsed = ((last.time_ms - first.time_ms) / DAY_MS) as u32;
            metrics::compute_cagr(first.index, last.index, days_elapsed)
        }
        _ => Err(metrics::CagrError::InsufficientSample {
            actual: 0,
            required: 365,
        }),
    };

    PerformanceSummary {
        daily_nav: history.daily_nav,
        daily_returns: history.daily_returns,
        sharpe,
        sortino,
        max_drawdown,
        cagr,
        win_rate: history.win_rate,
        earliest_reliable_checkpoint_ms: history.earliest_reliable_checkpoint_ms,
        pnl: history.pnl,
    }
}

#[cfg(test)]
mod series_tests {
    use super::*;

    fn candle(open: u64, close: u64, price: &str) -> RawCandle {
        RawCandle {
            open_time_ms: open,
            close_time_ms: close,
            close_price: price.to_string(),
        }
    }

    #[test]
    fn ranges_are_parsed_from_the_labels_the_chart_uses_and_nothing_else() {
        assert_eq!(SeriesRange::parse("24h"), Some(SeriesRange::Day));
        assert_eq!(SeriesRange::parse("7d"), Some(SeriesRange::Week));
        assert_eq!(SeriesRange::parse("30d"), Some(SeriesRange::Month));
        assert_eq!(SeriesRange::parse("1y"), Some(SeriesRange::Year));
        assert_eq!(SeriesRange::parse("5y"), Some(SeriesRange::FiveYears));
        assert_eq!(SeriesRange::parse("max"), Some(SeriesRange::Max));
        assert_eq!(SeriesRange::parse("2w"), None);
        assert_eq!(SeriesRange::parse(""), None);
    }

    #[test]
    fn fixed_ranges_use_their_own_resolution_and_max_adapts_to_the_age() {
        assert_eq!(step_for(SeriesRange::Day, DAY_MS), 300_000);
        assert_eq!(step_for(SeriesRange::Week, 7 * DAY_MS), 1_800_000);
        assert_eq!(step_for(SeriesRange::Month, 30 * DAY_MS), 7_200_000);
        assert_eq!(step_for(SeriesRange::Year, 365 * DAY_MS), DAY_MS);
        assert_eq!(step_for(SeriesRange::FiveYears, 5 * 365 * DAY_MS), DAY_MS);
        // Max: minutes of history are drawn per minute, months per day —
        // always a readable number of points.
        assert_eq!(step_for(SeriesRange::Max, 30 * 60_000), 60_000);
        for span in [3_600_000, 5 * 3_600_000, 3 * DAY_MS, 40 * DAY_MS, 400 * DAY_MS, 6 * 365 * DAY_MS] {
            let step = step_for(SeriesRange::Max, span);
            assert!(SERIES_STEPS_MS.contains(&step));
            assert!(span / step <= 400 || step == DAY_MS, "span {span} drew {} points", span / step);
        }
    }

    #[test]
    fn a_price_is_the_last_candle_already_closed_never_a_later_one() {
        let candles = vec![candle(0, 59_999, "100"), candle(60_000, 119_999, "110"), candle(120_000, 179_999, "120")];
        // Exactly on a boundary: the candle that just ended.
        assert_eq!(price_at(&candles, 120_000, false), Decimal::from_str("110").ok());
        // Mid-candle: still the last one that had closed by then.
        assert_eq!(price_at(&candles, 150_000, false), Decimal::from_str("110").ok());
        // Before any candle closed: no price, not an invented one.
        assert_eq!(price_at(&candles, 30_000, false), None);
        // "Now" is the live price of the newest candle.
        assert_eq!(price_at(&candles, 170_000, true), Decimal::from_str("120").ok());
    }
}
