//! Session verification, mirroring `services/collector/tlsn::verify`
//! — the same checks, run inside the zkVM this time so an A1
//! badge means the guest itself did the checking, not a caller's
//! say-so.

use crate::session::TlsSessionReceipt;
use k256::ecdsa::signature::Verifier;
use k256::ecdsa::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

pub const REQUIRED_DISCLOSED_CATEGORIES: [&str; 7] = [
    "wallet_selector",
    "symbol",
    "filter",
    "type",
    "time_window",
    "error",
    "relevant_count",
];

#[derive(Debug)]
pub enum SessionError {
    HostnameMismatch,
    RedactionHidesRequiredField,
    WalletBindingInvalid,
    RawNotBoundToInput,
}

pub fn verify_session_shape(
    receipt: &TlsSessionReceipt,
    expected_hostname: &str,
) -> Result<(), SessionError> {
    if receipt.server_name != expected_hostname {
        return Err(SessionError::HostnameMismatch);
    }
    for category in REQUIRED_DISCLOSED_CATEGORIES {
        if !receipt
            .disclosed_fields
            .iter()
            .any(|field| field == category)
        {
            return Err(SessionError::RedactionHidesRequiredField);
        }
    }
    Ok(())
}

pub fn verify_wallet_binding(
    receipt: &TlsSessionReceipt,
    verifying_key_sec1: &[u8],
) -> Result<(), SessionError> {
    let verifying_key = VerifyingKey::from_sec1_bytes(verifying_key_sec1)
        .map_err(|_| SessionError::WalletBindingInvalid)?;
    let signature = Signature::from_slice(&receipt.wallet_binding_signature)
        .map_err(|_| SessionError::WalletBindingInvalid)?;
    verifying_key
        .verify(&receipt.binding_digest(), &signature)
        .map_err(|_| SessionError::WalletBindingInvalid)?;
    Ok(())
}

/// The "raw TLS vs. normalized input swap" check: the bytes a
/// downstream calculation used must hash to exactly the response the
/// session captured.
pub fn verify_raw_bound_to_input(
    receipt: &TlsSessionReceipt,
    normalized_input_bytes: &[u8],
) -> Result<(), SessionError> {
    let actual_hash: [u8; 32] = Sha256::digest(normalized_input_bytes).into();
    if actual_hash != receipt.response_body_hash {
        return Err(SessionError::RawNotBoundToInput);
    }
    Ok(())
}
