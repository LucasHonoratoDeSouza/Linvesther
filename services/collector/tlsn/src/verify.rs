//! Session verification: the checks a "B1 homologation" POC can
//! run today — hostname compatibility, that redaction preserved the
//! categories proofs.md forbids hiding, wallet binding, and that the
//! raw bytes a downstream calculation used are the exact same bytes the
//! session captured.

use crate::session::TlsSessionReceipt;
use k256::ecdsa::signature::Verifier;
use k256::ecdsa::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

/// Fields proofs.md says redaction must never hide: "seletor de
/// carteira, símbolo, filtro, tipo, janela temporal, erro ou contagem
/// relevante."
pub const REQUIRED_DISCLOSED_CATEGORIES: [&str; 7] = [
    "wallet_selector",
    "symbol",
    "filter",
    "type",
    "time_window",
    "error",
    "relevant_count",
];

#[derive(Debug, PartialEq, Eq)]
pub enum SessionError {
    HostnameMismatch { expected: String, actual: String },
    RedactionHidesRequiredField { category: &'static str },
    WalletBindingInvalid,
    RawNotBoundToInput,
}

/// Checks hostname compatibility and that every required disclosure
/// category is present — a session that redacts even one of them is
/// not homologated, regardless of how many other fields it discloses.
pub fn verify_session_shape(
    receipt: &TlsSessionReceipt,
    expected_hostname: &str,
) -> Result<(), SessionError> {
    if receipt.server_name != expected_hostname {
        return Err(SessionError::HostnameMismatch {
            expected: expected_hostname.to_string(),
            actual: receipt.server_name.clone(),
        });
    }
    for category in REQUIRED_DISCLOSED_CATEGORIES {
        if !receipt
            .disclosed_fields
            .iter()
            .any(|field| field == category)
        {
            return Err(SessionError::RedactionHidesRequiredField { category });
        }
    }
    Ok(())
}

/// Verifies the wallet-binding signature covers this exact receipt —
/// any field changed after signing (including which categories were
/// disclosed) invalidates the binding.
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

/// The literal "falha se raw não é ligado ao input" requirement: the
/// bytes a downstream calculation actually used must hash to exactly
/// the same digest the session receipt captured. There is no tolerance
/// or partial match — any divergence means the calculation cannot be
/// said to have used what this session observed.
pub fn verify_raw_bound_to_input(
    receipt: &TlsSessionReceipt,
    actual_input_bytes: &[u8],
) -> Result<(), SessionError> {
    let actual_hash: [u8; 32] = Sha256::digest(actual_input_bytes).into();
    if actual_hash != receipt.response_body_hash {
        return Err(SessionError::RawNotBoundToInput);
    }
    Ok(())
}
