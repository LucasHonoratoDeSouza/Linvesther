//! Request signing for authenticated Binance REST endpoints.
//!
//! Isolated from transport so the signature can be verified deterministically
//! without a live connection or stored credentials.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, thiserror::Error)]
pub enum SigningError {
    #[error("secret key must not be empty")]
    EmptySecret,
}

/// Signs a canonical query string with the account's API secret.
///
/// Binance requires the raw, already-ordered query string (no additional
/// encoding) as the HMAC-SHA256 message; the resulting signature is appended
/// as a `signature` parameter by the caller.
pub fn sign_query(secret: &str, query: &str) -> Result<String, SigningError> {
    if secret.is_empty() {
        return Err(SigningError::EmptySecret);
    }
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(query.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}
