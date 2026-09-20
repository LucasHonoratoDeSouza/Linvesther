//! Binance balance-affecting flow normalization.
//!
//! Six endpoint families (deposit, withdrawal, universal transfer,
//! Convert, dust, dividend) each normalize into the same [`NormalizedFlow`]
//! shape. Every family fails closed on an unrecognized status/type/reason
//! rather than guessing — see each module's own doc comment for the
//! specific classification it applies. Reconciliation (fee accounted
//! exactly once, legs never double-counted) is structural: `fee` is a
//! separate field from `legs`, never folded into it.
//!
//! Does not call the Binance API itself — see `README.md` for what still
//! requires a real, credentialed run and for the transfer/dividend policy
//! inputs this crate deliberately does not hardcode.

pub mod common;
pub mod convert;
pub mod deposit;
pub mod dividend;
pub mod dust;
pub mod transfer;
pub mod withdrawal;

pub use common::{AssetLeg, FlowError, FlowFamily, NormalizedFlow};
pub use convert::{normalize_convert, RawConvert};
pub use deposit::{normalize_deposit, RawDeposit};
pub use dividend::{normalize_dividend, DividendReasonPolicy, RawDividend};
pub use dust::{normalize_dust, RawDust, RawDustDetail};
pub use transfer::{normalize_transfer, RawTransfer, TransferDirection, TransferTypePolicy};
pub use withdrawal::{normalize_withdrawal, RawWithdrawal};
