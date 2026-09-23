//! Kraken's request signing: `API-Sign` is the base64 of an HMAC-SHA512,
//! keyed with the base64-decoded API secret, over the request path followed
//! by the SHA-256 of `nonce + POST body`.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("the API key is empty")]
    EmptyKey,
    #[error("the API secret is not valid base64 — copy it exactly as Kraken showed it")]
    BadSecret,
}

/// An API key and its secret. The secret never appears in `Debug` output.
pub struct Credentials {
    api_key: String,
    secret: Vec<u8>,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials").field("api_key", &"…").field("secret", &"…").finish()
    }
}

impl Credentials {
    pub fn new(api_key: &str, api_secret: &str) -> Result<Self, AuthError> {
        let api_key = api_key.trim();
        if api_key.is_empty() {
            return Err(AuthError::EmptyKey);
        }
        let secret = STANDARD.decode(api_secret.trim()).map_err(|_| AuthError::BadSecret)?;
        if secret.is_empty() {
            return Err(AuthError::BadSecret);
        }
        Ok(Credentials { api_key: api_key.to_string(), secret })
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// The `API-Sign` header value for a POST to `path` with `post_data` as its body
    /// (which must start with the same `nonce=` given here).
    pub fn sign(&self, path: &str, nonce: &str, post_data: &str) -> String {
        let mut digest = Sha256::new();
        digest.update(nonce.as_bytes());
        digest.update(post_data.as_bytes());
        let mut mac = Hmac::<Sha512>::new_from_slice(&self.secret).expect("HMAC accepts a key of any length");
        mac.update(path.as_bytes());
        mac.update(&digest.finalize());
        STANDARD.encode(mac.finalize().into_bytes())
    }
}

static LAST_NONCE: AtomicU64 = AtomicU64::new(0);

/// A nonce that only ever grows: the current time in microseconds, or one more
/// than the last one handed out if the clock has not moved. Kraken refuses a
/// nonce that is not larger than the previous one used with the same key.
pub fn next_nonce() -> u64 {
    let micros = SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock is after the epoch").as_micros() as u64;
    let mut last = LAST_NONCE.load(Ordering::SeqCst);
    loop {
        let next = micros.max(last + 1);
        match LAST_NONCE.compare_exchange(last, next, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return next,
            Err(seen) => last = seen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The worked example from Kraken's REST authentication guide.
    const SECRET: &str = "kQH5HW/8p1uGOVjbgWA7FunAmGO8lsSUXNsu3eow76sz84Q18fWxnyRzBHCd3pd5nE9qa99HAZtuZuj6F1huXg==";

    #[test]
    fn signs_the_documented_example_exactly() {
        let credentials = Credentials::new("key", SECRET).unwrap();
        let post = "nonce=1616492376594&ordertype=limit&pair=XBTUSD&price=37500&type=buy&volume=1.25";
        assert_eq!(
            credentials.sign("/0/private/AddOrder", "1616492376594", post),
            "4/dpxb3iT4tp/ZCVEwSnEsLxx0bqyhLpdfOpc6fn7OR8+UClSV5n9E6aSS8MPtnRfp32bAb0nmbRn6H8ndwLUQ=="
        );
    }

    #[test]
    fn a_different_body_or_path_gives_a_different_signature() {
        let credentials = Credentials::new("key", SECRET).unwrap();
        let base = credentials.sign("/0/private/Balance", "1", "nonce=1");
        assert_ne!(base, credentials.sign("/0/private/Balance", "2", "nonce=2"));
        assert_ne!(base, credentials.sign("/0/private/BalanceEx", "1", "nonce=1"));
    }

    #[test]
    fn rejects_an_empty_key_and_a_secret_that_is_not_base64() {
        assert_eq!(Credentials::new("  ", SECRET).unwrap_err(), AuthError::EmptyKey);
        assert_eq!(Credentials::new("key", "not base64 !!").unwrap_err(), AuthError::BadSecret);
        assert_eq!(Credentials::new("key", "").unwrap_err(), AuthError::BadSecret);
    }

    #[test]
    fn debug_output_never_shows_the_secret() {
        let credentials = Credentials::new("my-public-key", SECRET).unwrap();
        let shown = format!("{credentials:?}");
        assert!(!shown.contains(SECRET) && !shown.contains("my-public-key"));
    }

    #[test]
    fn nonces_strictly_increase_even_when_asked_in_a_burst() {
        let nonces: Vec<u64> = (0..1000).map(|_| next_nonce()).collect();
        assert!(nonces.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
