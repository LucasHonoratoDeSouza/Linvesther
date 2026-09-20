//! Binance B0 conformance experiment.
//!
//! Decides whether evidence collected from Binance Spot endpoints is
//! sufficient to claim `POLICY_COMPLETE` coverage for the `binance-global-spot-v1`
//! profile, per the Binance adapter specification. Does not perform live requests;
//! see `README.md` for the split between what this experiment validates and
//! what requires a real, credentialed run.

pub mod coverage;
pub mod model;
pub mod pagination;
pub mod signing;

pub use coverage::{evaluate, CoverageReport, CoverageStatus};
pub use model::*;
