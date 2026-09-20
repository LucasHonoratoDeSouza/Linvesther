//! Binance credential vault and connection primitives.
//!
//! Scope: envelope-encrypted credential storage with a revoke-then-purge
//! lifecycle (`vault`), `apiRestrictions` read-only validation
//! (`restrictions`), account-context (UID/sub-account/environment)
//! binding pinning (`binding`), and host/path-confined request signing
//! (`signing`). Does not perform live HTTP requests to Binance — see
//! `README.md` for what still requires a real, credentialed run.

pub mod binding;
pub mod kms;
pub mod restrictions;
pub mod signing;
pub mod vault;

pub use binding::{AccountContext, BindingError, PinnedBinding};
pub use kms::{Kms, KmsError, LocalKms};
pub use restrictions::{validate_read_only, ApiRestrictions, RestrictionsError};
pub use signing::{host_for, redact_query_for_logging, sign_request, SigningError, ALLOWED_PATHS};
pub use vault::{ApiCredential, CredentialRecord, CredentialVault, Environment, VaultError};
