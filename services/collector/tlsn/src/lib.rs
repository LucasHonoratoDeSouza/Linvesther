//! TLS origin (A1) homologation POC. Per this task's own "Done
//! when," this is explicitly a POC that registers its limits — see
//! this crate's README — not a claim of full TLSNotary integration or
//! a real Binance B1 homologation (no real Binance TLS session has been
//! captured in this repository; see `services/collector/binance/*`'s
//! own READMEs for the same standing limitation on the A0 side).

pub mod session;
pub mod verify;

pub use session::TlsSessionReceipt;
pub use verify::{
    verify_raw_bound_to_input, verify_session_shape, verify_wallet_binding, SessionError,
    REQUIRED_DISCLOSED_CATEGORIES,
};
