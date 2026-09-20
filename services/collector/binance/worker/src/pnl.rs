//! Realized and unrealized profit per position, in USDT, **measured only
//! from the moment the account was connected**: what was already held
//! at that instant enters at its price *then* (an opening balance, not a
//! gain or a loss), and only what happens afterwards moves the numbers.
//!
//! Pure arithmetic over already-priced events — no I/O, no clock — so
//! every rule below is unit-tested directly. Average-cost accounting
//! (the "average buy price" people expect to see next to a position):
//!
//! * an acquisition (a buy, or a deposit valued at its market price
//!   when it arrived) raises the position's average cost;
//! * a disposal realizes `(sell price − average cost) × quantity`, and
//!   leaves the average cost of what remains unchanged;
//! * a withdrawal removes quantity at its average cost — moving your
//!   own assets out is neither a gain nor a loss;
//! * a disposal larger than what this history has ever seen acquired
//!   (a position opened before the connection, sold beyond its opening
//!   balance) realizes nothing for the unmatched part rather than
//!   inventing a cost basis for it — the same rule the win-rate uses.

use rust_decimal::Decimal;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PnlEvent {
    /// `quantity` of `asset` came in at `unit_price` USDT per unit.
    Acquire {
        asset: String,
        quantity: Decimal,
        unit_price: Decimal,
    },
    /// `quantity` of `asset` was sold at `unit_price` USDT per unit.
    Dispose {
        asset: String,
        quantity: Decimal,
        unit_price: Decimal,
    },
    /// `quantity` of `asset` left the account (not a sale).
    Withdraw { asset: String, quantity: Decimal },
    /// A trading or network fee, already valued in USDT.
    Fee { value: Decimal },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionPnl {
    pub asset: String,
    /// What the account actually holds now.
    pub quantity: Decimal,
    /// Average USDT cost per unit of what is held, `None` when nothing
    /// in this history was ever acquired for it.
    pub average_cost: Option<Decimal>,
    pub mark_price: Decimal,
    pub realized: Decimal,
    /// `(mark − average cost) × quantity` — `None` alongside a `None`
    /// cost (no basis to measure against).
    pub unrealized: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PnlSummary {
    pub positions: Vec<PositionPnl>,
    pub realized: Decimal,
    pub unrealized: Decimal,
    /// Every fee paid since the connection, in USDT (already excluded
    /// from `realized` — shown separately so nothing is hidden).
    pub fees: Decimal,
}

struct Lot {
    held: Decimal,
    average_cost: Decimal,
    realized: Decimal,
}

/// `openings` are `(asset, quantity, unit_price)` held at the instant of
/// connection; `events` must already be in chronological order;
/// `current_quantities`/`marks` describe the account now.
pub fn compute_pnl(
    openings: &[(String, Decimal, Decimal)],
    events: &[PnlEvent],
    current_quantities: &BTreeMap<String, Decimal>,
    marks: &BTreeMap<String, Decimal>,
) -> PnlSummary {
    let mut lots: BTreeMap<String, Lot> = BTreeMap::new();
    let mut fees = Decimal::ZERO;

    let acquire = |lots: &mut BTreeMap<String, Lot>, asset: &str, quantity: Decimal, price: Decimal| {
        let lot = lots.entry(asset.to_string()).or_insert(Lot {
            held: Decimal::ZERO,
            average_cost: Decimal::ZERO,
            realized: Decimal::ZERO,
        });
        let total = lot.held + quantity;
        if total > Decimal::ZERO {
            lot.average_cost = (lot.held * lot.average_cost + quantity * price) / total;
        }
        lot.held = total;
    };

    for (asset, quantity, price) in openings {
        acquire(&mut lots, asset, *quantity, *price);
    }

    for event in events {
        match event {
            PnlEvent::Acquire {
                asset,
                quantity,
                unit_price,
            } => acquire(&mut lots, asset, *quantity, *unit_price),
            PnlEvent::Dispose {
                asset,
                quantity,
                unit_price,
            } => {
                let lot = lots.entry(asset.clone()).or_insert(Lot {
                    held: Decimal::ZERO,
                    average_cost: Decimal::ZERO,
                    realized: Decimal::ZERO,
                });
                let matched = (*quantity).min(lot.held);
                lot.realized += matched * (*unit_price - lot.average_cost);
                lot.held -= matched;
            }
            PnlEvent::Withdraw { asset, quantity } => {
                if let Some(lot) = lots.get_mut(asset) {
                    lot.held -= (*quantity).min(lot.held);
                }
            }
            PnlEvent::Fee { value } => fees += *value,
        }
    }

    let mut positions = Vec::new();
    let mut realized_total = Decimal::ZERO;
    let mut unrealized_total = Decimal::ZERO;

    let mut assets: Vec<&String> = lots.keys().chain(current_quantities.keys()).collect();
    assets.sort();
    assets.dedup();
    for asset in assets {
        let quantity = current_quantities.get(asset).copied().unwrap_or(Decimal::ZERO);
        let lot = lots.get(asset);
        let realized = lot.map_or(Decimal::ZERO, |l| l.realized);
        if quantity == Decimal::ZERO && realized == Decimal::ZERO {
            continue;
        }
        let mark_price = marks.get(asset).copied().unwrap_or(Decimal::ZERO);
        let average_cost = lot.filter(|l| l.held > Decimal::ZERO).map(|l| l.average_cost);
        let unrealized = average_cost.map(|cost| (mark_price - cost) * quantity);
        realized_total += realized;
        unrealized_total += unrealized.unwrap_or(Decimal::ZERO);
        positions.push(PositionPnl {
            asset: asset.clone(),
            quantity,
            average_cost,
            mark_price,
            realized,
            unrealized,
        });
    }

    PnlSummary {
        positions,
        realized: realized_total,
        unrealized: unrealized_total,
        fees,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(value: &str) -> Decimal {
        Decimal::from_str(value).unwrap()
    }

    fn qty(pairs: &[(&str, Decimal)]) -> BTreeMap<String, Decimal> {
        pairs.iter().map(|(a, q)| (a.to_string(), *q)).collect()
    }

    fn acquire(asset: &str, quantity: Decimal, unit_price: Decimal) -> PnlEvent {
        PnlEvent::Acquire { asset: asset.into(), quantity, unit_price }
    }
    fn dispose(asset: &str, quantity: Decimal, unit_price: Decimal) -> PnlEvent {
        PnlEvent::Dispose { asset: asset.into(), quantity, unit_price }
    }

    #[test]
    fn what_was_held_at_connection_starts_flat_not_as_a_gain() {
        // 2 BTC held at connection when BTC was 100; it is 110 now.
        // The 100 is the baseline: only the +10/unit since then counts.
        let summary = compute_pnl(
            &[("BTC".into(), d("2"), d("100"))],
            &[],
            &qty(&[("BTC", d("2"))]),
            &qty(&[("BTC", d("110"))]),
        );
        assert_eq!(summary.realized, d("0"));
        assert_eq!(summary.unrealized, d("20"));
        assert_eq!(summary.positions[0].average_cost, Some(d("100")));
    }

    #[test]
    fn a_sale_realizes_against_the_average_cost_and_keeps_the_average() {
        // Hold 1 @ 100, buy 1 @ 200 => average 150. Sell 1 @ 180 => +30
        // realized; the remaining 1 still averages 150, marked at 190.
        let summary = compute_pnl(
            &[("ETH".into(), d("1"), d("100"))],
            &[acquire("ETH", d("1"), d("200")), dispose("ETH", d("1"), d("180"))],
            &qty(&[("ETH", d("1"))]),
            &qty(&[("ETH", d("190"))]),
        );
        assert_eq!(summary.realized, d("30"));
        assert_eq!(summary.unrealized, d("40"));
        assert_eq!(summary.positions[0].average_cost, Some(d("150")));
    }

    #[test]
    fn selling_more_than_this_history_ever_acquired_realizes_nothing_for_the_excess() {
        // Nothing opened or bought: an unmatched sell has no cost basis
        // to measure against, so it is not invented.
        let summary = compute_pnl(
            &[],
            &[dispose("SOL", d("5"), d("100"))],
            &qty(&[]),
            &qty(&[]),
        );
        assert_eq!(summary.realized, d("0"));
        assert!(summary.positions.is_empty());
    }

    #[test]
    fn a_deposit_enters_at_market_price_and_is_not_a_profit() {
        let summary = compute_pnl(
            &[],
            &[acquire("BTC", d("1"), d("100"))],
            &qty(&[("BTC", d("1"))]),
            &qty(&[("BTC", d("100"))]),
        );
        assert_eq!(summary.realized, d("0"));
        assert_eq!(summary.unrealized, d("0"));
    }

    #[test]
    fn a_withdrawal_is_neither_a_gain_nor_a_loss_and_keeps_the_average() {
        let summary = compute_pnl(
            &[("BTC".into(), d("2"), d("100"))],
            &[PnlEvent::Withdraw { asset: "BTC".into(), quantity: d("1") }],
            &qty(&[("BTC", d("1"))]),
            &qty(&[("BTC", d("150"))]),
        );
        assert_eq!(summary.realized, d("0"));
        assert_eq!(summary.unrealized, d("50"));
    }

    #[test]
    fn fees_are_summed_separately_and_never_hidden_inside_realized() {
        let summary = compute_pnl(
            &[],
            &[
                acquire("BTC", d("1"), d("100")),
                PnlEvent::Fee { value: d("0.1") },
                dispose("BTC", d("1"), d("110")),
                PnlEvent::Fee { value: d("0.11") },
            ],
            &qty(&[]),
            &qty(&[]),
        );
        assert_eq!(summary.realized, d("10"));
        assert_eq!(summary.fees, d("0.21"));
    }

    #[test]
    fn a_fully_closed_position_still_shows_its_realized_result() {
        let summary = compute_pnl(
            &[],
            &[acquire("BTC", d("1"), d("100")), dispose("BTC", d("1"), d("90"))],
            &qty(&[]),
            &qty(&[]),
        );
        assert_eq!(summary.positions.len(), 1);
        assert_eq!(summary.positions[0].realized, d("-10"));
        assert_eq!(summary.positions[0].unrealized, None);
    }
}
