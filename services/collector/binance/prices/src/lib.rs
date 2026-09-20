//! Authenticated valuation marks from Binance `klines`.
//!
//! Does not call the Binance API itself — see `README.md` for what still
//! requires a real, credentialed run.

pub mod candle;
pub mod mark;

pub use candle::Candle;
pub use mark::{mark_from_series, resolve_mark, select_candle, Mark, MarkError, PricePolicy};
