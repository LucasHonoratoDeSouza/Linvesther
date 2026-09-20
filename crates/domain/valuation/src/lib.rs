//! Valuation at a common cut: reconstructs each asset's quantity
//! from a non-atomic balance snapshot plus surrounding ledger deltas, then
//! computes `NAV(t) = Σ quantity(asset,t) × price(asset,t)`.

pub mod reconstruct;
pub mod valuation;

pub use reconstruct::{reconstruct_quantity, BalanceSnapshot, ReconstructError, TimedDelta};
pub use valuation::{value_at_cut, AssetInput, AssetValuation, Valuation, ValuationError};
