//! Real multi-user Binance connection worker: stores a read-only API
//! credential per account (encrypted at rest), and periodically syncs
//! trades/deposits/withdrawals/catalog into Postgres via
//! `binance-live-client`. This is the piece that makes "connect your
//! Binance account" work for any person, not just a one-off
//! homologation script — see `README.md` for what it still does not do.

pub mod coinbase;
pub mod crypto;
pub mod db;
pub mod history;
pub mod ibkr;
pub mod kraken;
pub mod market;
pub mod pnl;
#[cfg(feature = "proving")]
pub mod prove;
pub mod publish;
pub mod rekey;
pub mod secrets;
pub mod stream;
pub mod sync;
pub mod wallet;
