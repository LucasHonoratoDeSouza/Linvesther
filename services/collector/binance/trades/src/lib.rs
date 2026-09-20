//! Paginated `myTrades` collection.
//!
//! Does not call the Binance API itself — see `README.md` for what still
//! requires a real, credentialed run. This crate is the pagination and
//! namespace-safe storage logic a real collector calls per fetched page.

pub mod pagination;
pub mod store;
pub mod trade;

pub use pagination::{PageOutcome, PaginationTracker};
pub use store::{IngestError, TradeStore};
pub use trade::Trade;
