//! Simple Earn (flexible and locked savings, which is where staking of most coins sits).
//!
//! Funds moved into Earn leave the spot balance without being a deposit, a withdrawal or a
//! trade, so the account's value has to count the positions as well as the spot balance:
//! moving money between the two then changes nothing, and what Earn pays out is a reward
//! (performance), never capital brought in. This module only turns Binance's answers into
//! plain values; `LiveClient` makes the calls.

use crate::AccountBalance;
use binance_flows::{AssetLeg, FlowFamily, NormalizedFlow};
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::BTreeMap;
use std::str::FromStr;

/// How many results Binance returns per page at most.
pub const PAGE_SIZE: u64 = 100;
/// The longest span Binance answers a rewards query for.
pub const REWARDS_WINDOW_MS: u64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EarnKind {
    Flexible,
    Locked,
}

/// An amount currently held in Earn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EarnPosition {
    pub kind: EarnKind,
    pub asset: String,
    pub amount: String,
}

/// One page of a listing: its rows, and how many there are in all.
pub struct Page<T> {
    pub rows: Vec<T>,
    pub total: u64,
}

fn text(row: &Value, key: &str) -> Option<String> {
    row.get(key)?.as_str().map(str::to_string)
}

fn decimal(value: &str) -> Option<Decimal> {
    Decimal::from_str(value).ok()
}

/// A page of `/sapi/v1/simple-earn/flexible/position` (`totalAmount` per asset) or
/// `/locked/position` (`amount` per position). A row that is not readable is an error, not
/// a position quietly left out of the account's value.
pub fn parse_positions(body: &Value, kind: EarnKind) -> Result<Page<EarnPosition>, String> {
    let field = match kind {
        EarnKind::Flexible => "totalAmount",
        EarnKind::Locked => "amount",
    };
    let rows = body
        .get("rows")
        .and_then(Value::as_array)
        .ok_or("no rows in the answer")?;
    let mut positions = Vec::with_capacity(rows.len());
    for row in rows {
        let asset = text(row, "asset").ok_or("a position has no asset")?;
        let amount = text(row, field)
            .filter(|a| decimal(a).is_some())
            .ok_or_else(|| format!("a {asset} position has no readable amount"))?;
        positions.push(EarnPosition {
            kind,
            asset,
            amount,
        });
    }
    let total = body
        .get("total")
        .and_then(Value::as_u64)
        .unwrap_or(rows.len() as u64);
    Ok(Page {
        rows: positions,
        total,
    })
}

/// Rewards Earn paid, as credits that count as performance. `flexible` rows carry
/// `asset`, `rewards`, `projectId`, `type` and `time`; locked rows `asset`, `amount`,
/// `positionId`, `time` and `type`. The id is built from everything that tells two payments
/// apart, so reading the same window twice stores nothing twice.
pub fn parse_rewards(body: &Value, kind: EarnKind) -> Result<Page<NormalizedFlow>, String> {
    let rows = body
        .get("rows")
        .and_then(Value::as_array)
        .ok_or("no rows in the answer")?;
    let mut flows = Vec::with_capacity(rows.len());
    for row in rows {
        let asset = text(row, "asset").ok_or("a reward has no asset")?;
        let time = row
            .get("time")
            .and_then(Value::as_u64)
            .ok_or("a reward has no time")?;
        let (amount, id, label) = match kind {
            EarnKind::Flexible => {
                let reward_type = text(row, "type").unwrap_or_default();
                let project = text(row, "projectId").unwrap_or_default();
                (
                    text(row, "rewards"),
                    format!("earn:flexible:{reward_type}:{project}:{asset}:{time}"),
                    format!("Simple Earn flexible {reward_type}"),
                )
            }
            EarnKind::Locked => {
                let position = row
                    .get("positionId")
                    .map(|p| p.to_string())
                    .unwrap_or_default();
                (
                    text(row, "amount"),
                    format!("earn:locked:{position}:{asset}:{time}"),
                    "Simple Earn locked reward".to_string(),
                )
            }
        };
        let amount = amount
            .filter(|a| decimal(a).is_some())
            .ok_or_else(|| format!("a {asset} reward has no readable amount"))?;
        if decimal(&amount) == Some(Decimal::ZERO) {
            continue;
        }
        flows.push(NormalizedFlow {
            source_namespace: FlowFamily::Dividend,
            source_id: id,
            economic_time_ms: time,
            kind: label,
            legs: vec![AssetLeg::credit(asset, amount)],
            fee: None,
        });
    }
    let total = body
        .get("total")
        .and_then(Value::as_u64)
        .unwrap_or(rows.len() as u64);
    Ok(Page { rows: flows, total })
}

/// The spot balances plus what is held in Earn, one line per asset. Earn amounts count as
/// locked (not freely spendable). Flexible Earn used to show up in spot as an `LD`-prefixed
/// token (`LDBNB` for BNB); when the positions themselves are read, that token is the same
/// money and is left out so it is not counted twice.
pub fn with_earn(
    spot: Vec<AccountBalance>,
    positions: &[EarnPosition],
) -> Result<Vec<AccountBalance>, String> {
    let earned: BTreeMap<&str, ()> = positions.iter().map(|p| (p.asset.as_str(), ())).collect();
    let mut by_asset: BTreeMap<String, (Decimal, Decimal)> = BTreeMap::new();
    for balance in spot {
        if let Some(underlying) = balance.asset.strip_prefix("LD") {
            if earned.contains_key(underlying) {
                continue;
            }
        }
        let free = decimal(&balance.free)
            .ok_or_else(|| format!("{} balance is not a number", balance.asset))?;
        let locked = decimal(&balance.locked)
            .ok_or_else(|| format!("{} balance is not a number", balance.asset))?;
        let entry = by_asset.entry(balance.asset).or_default();
        entry.0 += free;
        entry.1 += locked;
    }
    for position in positions {
        let amount = decimal(&position.amount)
            .ok_or_else(|| format!("{} position is not a number", position.asset))?;
        by_asset.entry(position.asset.clone()).or_default().1 += amount;
    }
    Ok(by_asset
        .into_iter()
        .map(|(asset, (free, locked))| AccountBalance {
            asset,
            free: free.to_string(),
            locked: locked.to_string(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn balance(asset: &str, free: &str, locked: &str) -> AccountBalance {
        AccountBalance {
            asset: asset.into(),
            free: free.into(),
            locked: locked.into(),
        }
    }

    #[test]
    fn a_flexible_position_is_its_total_amount_and_a_locked_one_its_amount() {
        let flexible = parse_positions(
            &json!({"rows": [{"asset": "AXS", "totalAmount": "10.5"}], "total": 1}),
            EarnKind::Flexible,
        )
        .unwrap();
        assert_eq!(
            flexible.rows,
            vec![EarnPosition {
                kind: EarnKind::Flexible,
                asset: "AXS".into(),
                amount: "10.5".into()
            }]
        );
        let locked = parse_positions(&json!({"rows": [{"asset": "AXS", "amount": "122.09202928", "positionId": 1}], "total": 3}), EarnKind::Locked).unwrap();
        assert_eq!(
            (locked.rows[0].amount.as_str(), locked.total),
            ("122.09202928", 3)
        );
    }

    #[test]
    fn a_position_that_cannot_be_read_fails_instead_of_being_left_out() {
        assert!(parse_positions(
            &json!({"rows": [{"asset": "AXS", "totalAmount": "many"}]}),
            EarnKind::Flexible
        )
        .is_err());
        assert!(
            parse_positions(&json!({"rows": [{"totalAmount": "1"}]}), EarnKind::Flexible).is_err()
        );
        assert!(parse_positions(&json!({"nothing": true}), EarnKind::Locked).is_err());
    }

    #[test]
    fn earn_counts_toward_the_holding_so_moving_money_in_changes_nothing() {
        let before = with_earn(vec![balance("AXS", "100", "0")], &[]).unwrap();
        let after = with_earn(
            vec![balance("AXS", "0", "0")],
            &[EarnPosition {
                kind: EarnKind::Flexible,
                asset: "AXS".into(),
                amount: "100".into(),
            }],
        )
        .unwrap();
        let total = |b: &[AccountBalance]| {
            b.iter()
                .map(|x| decimal(&x.free).unwrap() + decimal(&x.locked).unwrap())
                .sum::<Decimal>()
        };
        assert_eq!(total(&before), total(&after));
    }

    #[test]
    fn several_positions_of_one_asset_add_up_with_the_spot_balance() {
        let positions = [
            EarnPosition {
                kind: EarnKind::Flexible,
                asset: "AXS".into(),
                amount: "1.5".into(),
            },
            EarnPosition {
                kind: EarnKind::Locked,
                asset: "AXS".into(),
                amount: "2".into(),
            },
            EarnPosition {
                kind: EarnKind::Locked,
                asset: "AXS".into(),
                amount: "3".into(),
            },
        ];
        let merged = with_earn(
            vec![balance("AXS", "4", "0.5"), balance("USDT", "9", "0")],
            &positions,
        )
        .unwrap();
        let axs = merged.iter().find(|b| b.asset == "AXS").unwrap();
        assert_eq!(
            (decimal(&axs.free).unwrap(), decimal(&axs.locked).unwrap()),
            (Decimal::from(4), Decimal::from_str("7.0").unwrap())
        );
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn a_flexible_earn_token_in_spot_is_not_counted_on_top_of_its_position() {
        let merged = with_earn(
            vec![balance("LDAXS", "100", "0")],
            &[EarnPosition {
                kind: EarnKind::Flexible,
                asset: "AXS".into(),
                amount: "100".into(),
            }],
        )
        .unwrap();
        assert_eq!(
            merged.iter().map(|b| b.asset.as_str()).collect::<Vec<_>>(),
            vec!["AXS"]
        );
    }

    #[test]
    fn a_real_token_that_merely_starts_with_ld_is_kept() {
        let merged = with_earn(
            vec![balance("LDO", "5", "0")],
            &[EarnPosition {
                kind: EarnKind::Flexible,
                asset: "AXS".into(),
                amount: "1".into(),
            }],
        )
        .unwrap();
        assert!(merged.iter().any(|b| b.asset == "LDO" && b.free == "5"));
    }

    #[test]
    fn rewards_are_credits_that_count_as_performance_and_never_as_capital() {
        let body = json!({"rows": [{"asset": "AXS", "rewards": "0.5", "projectId": "AXS001", "type": "REALTIME", "time": 1_000}], "total": 1});
        let flow = &parse_rewards(&body, EarnKind::Flexible).unwrap().rows[0];
        assert_eq!(flow.source_namespace, FlowFamily::Dividend);
        assert_eq!(flow.legs, vec![AssetLeg::credit("AXS", "0.5")]);
        assert_eq!(flow.economic_time_ms, 1_000);
        let locked = json!({"rows": [{"positionId": 7, "time": 2_000, "asset": "AXS", "amount": "1.25", "type": "Locked Rewards"}], "total": 1});
        assert_eq!(
            parse_rewards(&locked, EarnKind::Locked).unwrap().rows[0].legs,
            vec![AssetLeg::credit("AXS", "1.25")]
        );
    }

    #[test]
    fn two_payments_are_never_given_the_same_id_and_the_same_one_always_is() {
        let row = |t, project: &str| json!({"asset": "AXS", "rewards": "1", "projectId": project, "type": "BONUS", "time": t});
        let rows = json!({"rows": [row(1, "A"), row(2, "A"), row(1, "B")], "total": 3});
        let ids: Vec<_> = parse_rewards(&rows, EarnKind::Flexible)
            .unwrap()
            .rows
            .into_iter()
            .map(|f| f.source_id)
            .collect();
        assert_eq!(
            ids.iter().collect::<std::collections::BTreeSet<_>>().len(),
            3
        );
        let again: Vec<_> = parse_rewards(&rows, EarnKind::Flexible)
            .unwrap()
            .rows
            .into_iter()
            .map(|f| f.source_id)
            .collect();
        assert_eq!(ids, again);
    }

    #[test]
    fn a_zero_reward_is_skipped_and_an_unreadable_one_fails() {
        let zero = json!({"rows": [{"asset": "AXS", "rewards": "0.00000000", "projectId": "P", "type": "BONUS", "time": 1}], "total": 1});
        assert!(parse_rewards(&zero, EarnKind::Flexible)
            .unwrap()
            .rows
            .is_empty());
        let bad = json!({"rows": [{"asset": "AXS", "rewards": "x", "projectId": "P", "type": "BONUS", "time": 1}]});
        assert!(parse_rewards(&bad, EarnKind::Flexible).is_err());
    }
}
