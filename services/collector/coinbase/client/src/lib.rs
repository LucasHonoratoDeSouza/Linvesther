//! Read-only access to a Coinbase (Advanced Trade) account with a CDP API
//! key: balances, fills and prices — everything the performance engine
//! needs, behind [`exchange_core::MarketData`].

pub mod auth;
mod client;
pub mod wire;

pub use auth::{AuthError, Credentials};
pub use client::{CoinbaseClient, CoinbaseError, QUOTE_CURRENCY};
