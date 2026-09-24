//! Authenticated request signing, confined to a fixed host and an
//! explicit path allowlist.
//!
//! Per the Binance adapter specification: "Assinaturas HMAC de request ficam
//! dentro do coletor. Hostname fixo e allowlist de paths; nunca aceitar
//! URL arbitrária. Redigir API keys, signatures e query strings sensíveis
//! em logs." This module never accepts a caller-supplied base URL — the
//! host is fixed per [`crate::vault::Environment`], and every request path
//! must be in [`ALLOWED_PATHS`].

use crate::vault::Environment;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub const PRODUCTION_HOST: &str = "https://api.binance.com";
pub const TEST_HOST: &str = "https://testnet.binance.vision";

/// Paths this collector is authorized to call, per the endpoint table in
/// the Binance adapter specification. Anything else is refused before a
/// request is ever built — this is not a runtime-configurable list.
pub const ALLOWED_PATHS: &[&str] = &[
    "/api/v3/account",
    "/sapi/v1/account/apiRestrictions",
    "/api/v3/myTrades",
    "/api/v3/exchangeInfo",
    "/sapi/v1/capital/deposit/hisrec",
    "/sapi/v1/capital/withdraw/history",
    "/sapi/v1/asset/transfer",
    "/sapi/v1/convert/tradeFlow",
    "/sapi/v1/asset/dribblet",
    "/sapi/v1/asset/assetDividend",
    "/sapi/v1/simple-earn/flexible/position",
    "/sapi/v1/simple-earn/locked/position",
    "/sapi/v1/simple-earn/flexible/history/rewardsRecord",
    "/sapi/v1/simple-earn/locked/history/rewardsRecord",
    "/api/v3/klines",
];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SigningError {
    #[error("path not in the collector's allowlist: {0}")]
    PathNotAllowed(String),
    #[error("secret must not be empty")]
    EmptySecret,
}

/// Resolves the fixed host for `environment`. There is no way to pass a
/// different host in: this function's return value is the only host this
/// module will ever build a request against.
pub fn host_for(environment: Environment) -> &'static str {
    match environment {
        Environment::Production => PRODUCTION_HOST,
        Environment::Test => TEST_HOST,
    }
}

/// Signs `query` (the canonical, already-ordered query string) with
/// `secret`, after checking `path` against [`ALLOWED_PATHS`]. Returns the
/// hex-encoded signature to append as the `signature` query parameter.
pub fn sign_request(secret: &str, path: &str, query: &str) -> Result<String, SigningError> {
    if !ALLOWED_PATHS.contains(&path) {
        return Err(SigningError::PathNotAllowed(path.to_string()));
    }
    if secret.is_empty() {
        return Err(SigningError::EmptySecret);
    }
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(query.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

/// Redacts API keys, signatures and full query strings from a string
/// intended for logs, per the spec's explicit instruction. This is a
/// blunt, conservative redaction (replaces the entire query string), not
/// a selective field-by-field filter — logging code should prefer not to
/// include the query string at all and call this only as a last resort.
pub fn redact_query_for_logging(query: &str) -> String {
    if query.is_empty() {
        "[empty]".to_string()
    } else {
        format!("[redacted {} bytes]", query.len())
    }
}
