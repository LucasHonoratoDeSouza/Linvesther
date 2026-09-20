//! Time-weighted return index.

pub mod twr;

pub use twr::{compute_twr, ReturnError, TimelineEvent, TwrResult};
