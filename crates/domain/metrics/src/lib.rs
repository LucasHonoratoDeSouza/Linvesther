//! Risk and capital metrics: MDD/recovery, mean/variance/
//! volatility/Sharpe/Sortino with enforced minimum samples, CAGR (point
//! estimate — see `cagr` module's documented limitation), current and
//! sustained capital, and consistency.

pub mod cagr;
pub mod capital;
pub mod drawdown;
pub mod stats;
pub mod winrate;

pub use cagr::{cagr as compute_cagr, CagrError};
pub use capital::{consistency, current_capital, sustained_capital, Consistency, CurrentCapital};
pub use drawdown::{max_drawdown, recovery_time, DrawdownEpisode, IndexPoint, RecoveryStatus};
pub use stats::{annualized_volatility, mean, sample_variance, sharpe, sortino, StatsError};
pub use winrate::{compute_win_rate, Fill, WinRateError, WinRateResult};
