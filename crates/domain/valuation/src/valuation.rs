//! `NAV(t) = Σ quantity(asset,t) × price(asset,t)` in the spot profile,
//! per the protocol specification.

use crate::reconstruct::{reconstruct_quantity, BalanceSnapshot, ReconstructError, TimedDelta};
use rust_decimal::Decimal;
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ValuationError {
    #[error("asset {asset}: {source}")]
    AmbiguousOrdering {
        asset: String,
        #[source]
        source: ReconstructError,
    },
    #[error("asset {0} has no price support at this cut")]
    UnsupportedAsset(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetValuation {
    pub asset: String,
    pub quantity: Decimal,
    pub price: Decimal,
    pub value: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Valuation {
    pub cut_ms: u64,
    pub assets: Vec<AssetValuation>,
    pub nav: Decimal,
}

/// One asset's input to the valuation: its balance snapshot and the
/// ledger deltas observed around it (for common-cut reconstruction).
pub struct AssetInput<'a> {
    pub asset: String,
    pub snapshot: BalanceSnapshot,
    pub events: &'a [TimedDelta],
}

/// Values every asset in `inputs` at `cut_ms` using `prices` (asset →
/// price at the cut; obtaining that price, e.g. from an authenticated
/// mark, is the caller's concern — this function only consumes it).
/// Refuses the *entire* valuation (not a partial result skipping the bad
/// asset) if any single asset has an ambiguous reconstruction or no price
/// entry: a NAV missing one asset's contribution is not a smaller-but-valid
/// NAV, it is not a valid NAV at all.
pub fn value_at_cut(
    inputs: &[AssetInput],
    prices: &BTreeMap<String, Decimal>,
    cut_ms: u64,
) -> Result<Valuation, ValuationError> {
    let mut assets = Vec::with_capacity(inputs.len());
    let mut nav = Decimal::ZERO;

    for input in inputs {
        let quantity =
            reconstruct_quantity(&input.snapshot, input.events, cut_ms).map_err(|source| {
                ValuationError::AmbiguousOrdering {
                    asset: input.asset.clone(),
                    source,
                }
            })?;

        let price = prices
            .get(&input.asset)
            .copied()
            .ok_or_else(|| ValuationError::UnsupportedAsset(input.asset.clone()))?;

        let value = quantity * price;
        nav += value;
        assets.push(AssetValuation {
            asset: input.asset.clone(),
            quantity,
            price,
            value,
        });
    }

    Ok(Valuation {
        cut_ms,
        assets,
        nav,
    })
}
