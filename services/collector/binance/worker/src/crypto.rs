//! Symmetric encryption-at-rest for stored API credentials.
//!
//! AES-256-GCM, same primitive `collector_credentials::vault` already
//! uses — this module does not reuse that crate's `CredentialVault`
//! directly because its storage is in-memory by design (see its own
//! doc comment: "a real deployment backs this with a database, but
//! that persistence layer is a separate concern"). This is that
//! persistence layer, for this one connection type.
//!
//! **Known limitation, documented not hidden**: the master key comes
//! from a single symmetric key in the environment
//! (`BINANCE_WORKER_ENCRYPTION_KEY`), not a real KMS. This mirrors
//! `collector_credentials::kms::LocalKms`, which is explicitly
//! documented as "not a production KMS" — the same gap, not a new one.
//! A real deployment wires a real `Kms` implementation here the same
//! way it would for `collector_credentials::vault`.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("master key must be exactly 32 bytes, got {0}")]
    WrongKeyLength(usize),
    #[error("master key is not valid hex: {0}")]
    InvalidHex(String),
    #[error("encryption failed")]
    Encrypt,
    #[error("decryption failed — wrong key or tampered ciphertext")]
    Decrypt,
}

pub struct MasterKey([u8; 32]);

impl MasterKey {
    /// Parses a 64-character hex string into a 32-byte key. Generate
    /// one with `openssl rand -hex 32`.
    pub fn from_hex(hex_str: &str) -> Result<Self, CryptoError> {
        let bytes = hex::decode(hex_str).map_err(|e| CryptoError::InvalidHex(e.to_string()))?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|v: Vec<u8>| CryptoError::WrongKeyLength(v.len()))?;
        Ok(MasterKey(arr))
    }
}

pub struct EncryptedField {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12],
}

pub fn encrypt(key: &MasterKey, plaintext: &str) -> Result<EncryptedField, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(&key.0).map_err(|_| CryptoError::Encrypt)?;
    let mut nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut nonce_bytes).map_err(|_| CryptoError::Encrypt)?;
    let nonce = Nonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| CryptoError::Encrypt)?;
    Ok(EncryptedField {
        ciphertext,
        nonce: nonce_bytes,
    })
}

pub fn decrypt(
    key: &MasterKey,
    ciphertext: &[u8],
    nonce_bytes: &[u8; 12],
) -> Result<String, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(&key.0).map_err(|_| CryptoError::Decrypt)?;
    let nonce = Nonce::from(*nonce_bytes);
    let plaintext = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| CryptoError::Decrypt)?;
    String::from_utf8(plaintext).map_err(|_| CryptoError::Decrypt)
}
