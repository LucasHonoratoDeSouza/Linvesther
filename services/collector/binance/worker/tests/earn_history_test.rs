//! What Simple Earn does to the account's history, through the real engine and the real
//! merge of spot with Earn holdings — no exchange and no database involved.

use binance_flows::{AssetLeg, FlowFamily, NormalizedFlow};
use binance_live_client::earn::{with_earn, EarnKind, EarnPosition};
use binance_live_client::AccountBalance;
use binance_worker::history::history_from;
use exchange_core::{MarketData, MarketError, RawCandle};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::Arc;

const DAY: u64 = 86_400_000;
const START: u64 = 1_700_000_000_000 / DAY * DAY;

/// A market where NICHE always costs 10 USDT and USDT costs 1, holding what it is given.
struct Market(Vec<AccountBalance>);

impl MarketData for Market {
    fn quote_currency(&self) -> &str {
        "USDT"
    }
    fn market_symbol(&self, asset: &str) -> String {
        format!("{asset}USDT")
    }
    fn fetch_symbol_assets_for(
        &self,
        symbols: &BTreeSet<String>,
    ) -> Result<BTreeMap<String, (String, String)>, MarketError> {
        Ok(symbols
            .iter()
            .filter(|s| *s == "NICHEUSDT")
            .map(|s| (s.clone(), ("NICHE".to_string(), "USDT".to_string())))
            .collect())
    }
    fn fetch_account_balances(&self) -> Result<Vec<AccountBalance>, MarketError> {
        Ok(self.0.clone())
    }
    fn fetch_candles_span(
        &self,
        _: &str,
        _: u64,
        _: u64,
        _: u64,
    ) -> Result<Vec<RawCandle>, MarketError> {
        Ok(Vec::new())
    }
    fn price_at_instant(&self, asset: &str, _: u64) -> Result<Decimal, MarketError> {
        Ok(if asset == "NICHE" {
            Decimal::from(10)
        } else {
            Decimal::ONE
        })
    }
}

fn flow(family: FlowFamily, id: &str, time: u64, amount: &str) -> NormalizedFlow {
    NormalizedFlow {
        source_namespace: family,
        source_id: id.into(),
        economic_time_ms: time,
        kind: "test".into(),
        legs: vec![AssetLeg::credit("NICHE", amount)],
        fee: None,
    }
}

fn spot(free: &str) -> Vec<AccountBalance> {
    vec![AccountBalance {
        asset: "NICHE".into(),
        free: free.into(),
        locked: "0".into(),
    }]
}

fn earn(amount: &str) -> Vec<EarnPosition> {
    vec![EarnPosition {
        kind: EarnKind::Flexible,
        asset: "NICHE".into(),
        amount: amount.into(),
    }]
}

async fn final_index(balances: Vec<AccountBalance>, flows: Vec<NormalizedFlow>) -> Option<Decimal> {
    let history = history_from(
        Vec::new(),
        flows,
        START,
        Arc::new(Market(balances)),
        START + 5 * DAY,
    )
    .await
    .ok()?;
    Some(history.twr_index.last()?.index)
}

// In each case the account held 100 NICHE when it was connected, and by now everything it
// holds sits in Simple Earn.

#[tokio::test]
async fn moving_everything_into_earn_is_not_a_loss() {
    let in_earn = with_earn(spot("0"), &earn("100")).unwrap();
    assert_eq!(final_index(in_earn, Vec::new()).await, Some(Decimal::ONE));
    // Not counting Earn, the same account has nothing left to measure.
    assert_eq!(final_index(spot("0"), Vec::new()).await, None);
}

#[tokio::test]
async fn what_earn_pays_is_performance() {
    let reward = flow(
        FlowFamily::Dividend,
        "earn:flexible:REALTIME:P:NICHE:1",
        START + 2 * DAY,
        "1",
    );
    let holding = with_earn(spot("0"), &earn("101")).unwrap();
    assert_eq!(
        final_index(holding.clone(), vec![reward]).await,
        Some(Decimal::from_str("1.01").unwrap())
    );
    // With the reward unrecorded the gain disappears: it is what reading the rewards is for.
    assert_eq!(final_index(holding, Vec::new()).await, Some(Decimal::ONE));
}

#[tokio::test]
async fn putting_more_money_in_later_is_a_deposit_not_a_gain() {
    let more = flow(FlowFamily::Deposit, "d2", START + 2 * DAY, "50");
    let holding = with_earn(spot("0"), &earn("150")).unwrap();
    assert_eq!(
        final_index(holding.clone(), vec![more.clone()]).await,
        Some(Decimal::ONE)
    );
    assert_eq!(final_index(spot("0"), vec![more]).await, None);
}
