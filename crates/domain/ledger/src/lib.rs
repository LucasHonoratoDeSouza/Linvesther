//! Canonical ledger: reconciles per-asset balances from
//! adapter-normalized events, rejecting conflicting re-ingests and
//! counting fees exactly once.

pub mod event;
pub mod ledger;

pub use event::{LedgerEvent, LedgerLeg};
pub use ledger::{Ledger, LedgerError};
