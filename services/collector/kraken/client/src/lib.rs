//! Read-only access to a Kraken (spot) account with an API key: balances,
//! trades, the account ledger and prices — everything the performance engine
//! needs, behind [`exchange_core::MarketData`].

pub mod auth;
mod client;
pub mod names;
pub mod wire;

pub use auth::{AuthError, Credentials};
pub use client::{KrakenClient, KrakenError, LedgerEntry, Trade, QUOTE_CURRENCY};
