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

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use sha2::{Digest, Sha256};

/// Marks a ciphertext written by the current format. Older rows carry the raw
/// AES-GCM output with no marker; they are still read (see `decrypt`).
const FORMAT_V2: u8 = 2;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("master key must be exactly 32 bytes, got {0}")]
    WrongKeyLength(usize),
    #[error("master key is not valid hex: {0}")]
    InvalidHex(String),
    #[error("encryption failed")]
    Encrypt,
    #[error("decryption failed — wrong key, wrong account or tampered ciphertext")]
    Decrypt,
}

/// The master key and, while one is being rotated out, the keys it replaces.
/// New data is always written with `current`; anything written under a
/// previous key is still read, and is re-encrypted by `rekey`.
pub struct MasterKey {
    current: [u8; 32],
    previous: Vec<[u8; 32]>,
}

fn parse_key(hex_str: &str) -> Result<[u8; 32], CryptoError> {
    let bytes = hex::decode(hex_str.trim()).map_err(|e| CryptoError::InvalidHex(e.to_string()))?;
    bytes.try_into().map_err(|v: Vec<u8>| CryptoError::WrongKeyLength(v.len()))
}

/// A one-byte label for a key, stored beside the ciphertext so the right key
/// is tried first. It reveals nothing useful about the key.
fn key_id(key: &[u8; 32]) -> u8 {
    Sha256::digest(key)[0]
}

impl MasterKey {
    /// Parses a 64-character hex string into a 32-byte key. Generate
    /// one with `openssl rand -hex 32`.
    pub fn from_hex(hex_str: &str) -> Result<Self, CryptoError> {
        Ok(MasterKey { current: parse_key(hex_str)?, previous: Vec::new() })
    }

    /// Adds the keys this one replaces (comma-separated hex), so data written
    /// under them can still be read until it has been re-encrypted.
    pub fn with_previous(mut self, hex_list: &str) -> Result<Self, CryptoError> {
        for item in hex_list.split(',').map(str::trim).filter(|item| !item.is_empty()) {
            self.previous.push(parse_key(item)?);
        }
        Ok(self)
    }

    fn all(&self) -> impl Iterator<Item = &[u8; 32]> {
        std::iter::once(&self.current).chain(self.previous.iter())
    }
}

pub struct EncryptedField {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12],
}

/// Binds a stored secret to the account and field it belongs to, so a
/// ciphertext copied onto another row does not decrypt there.
pub fn credential_context(broker: &str, account_id: &str, field: &str) -> String {
    format!("lzk-credential/v2|{broker}|{account_id}|{field}")
}

fn seal(key: &[u8; 32], nonce: &[u8; 12], msg: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::Encrypt)?;
    cipher.encrypt(&Nonce::from(*nonce), Payload { msg, aad }).map_err(|_| CryptoError::Encrypt)
}

fn open(key: &[u8; 32], nonce: &[u8; 12], msg: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    cipher.decrypt(&Nonce::from(*nonce), Payload { msg, aad }).ok()
}

/// Encrypts with the current key, bound to `context`.
pub fn encrypt(key: &MasterKey, plaintext: &str, context: &str) -> Result<EncryptedField, CryptoError> {
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut nonce).map_err(|_| CryptoError::Encrypt)?;
    let sealed = seal(&key.current, &nonce, plaintext.as_bytes(), context.as_bytes())?;
    let mut ciphertext = Vec::with_capacity(sealed.len() + 2);
    ciphertext.push(FORMAT_V2);
    ciphertext.push(key_id(&key.current));
    ciphertext.extend_from_slice(&sealed);
    Ok(EncryptedField { ciphertext, nonce })
}

/// Decrypts what `encrypt` wrote, and, so nothing already stored is lost, what
/// the earlier format wrote (no marker, no context). A current-format value is
/// only accepted for the `context` it was written for.
pub fn decrypt(key: &MasterKey, ciphertext: &[u8], nonce: &[u8; 12], context: &str) -> Result<String, CryptoError> {
    if ciphertext.len() > 2 && ciphertext[0] == FORMAT_V2 {
        let wanted = ciphertext[1];
        // The labelled key first; the label is only a hint, so all are tried.
        let ordered = key.all().filter(|k| key_id(k) == wanted).chain(key.all().filter(|k| key_id(k) != wanted));
        for candidate in ordered {
            if let Some(plain) = open(candidate, nonce, &ciphertext[2..], context.as_bytes()) {
                return String::from_utf8(plain).map_err(|_| CryptoError::Decrypt);
            }
        }
    }
    // An earlier-format value can begin with the marker byte by chance, so this runs after the above fails too.
    for candidate in key.all() {
        if let Some(plain) = open(candidate, nonce, ciphertext, b"") {
            return String::from_utf8(plain).map_err(|_| CryptoError::Decrypt);
        }
    }
    Err(CryptoError::Decrypt)
}

/// Whether a stored value is already in the current format under the current
/// key, for `context`. Anything else should be re-encrypted. Checked by
/// decrypting with the current key alone, not by its one-byte label, so a
/// previous key that happens to share the label is never mistaken for it.
pub fn is_current(key: &MasterKey, ciphertext: &[u8], nonce: &[u8; 12], context: &str) -> bool {
    ciphertext.len() > 2 && ciphertext[0] == FORMAT_V2 && open(&key.current, nonce, &ciphertext[2..], context.as_bytes()).is_some()
}
