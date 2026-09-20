//! Envelope-encrypted credential storage with revoke-then-purge lifecycle.
//!
//! Per the operations design: "Remoção revoga
//! acesso e elimina credencial em até 24h." [`CredentialVault::revoke`]
//! makes the credential immediately unusable ([`retrieve`](CredentialVault::retrieve)
//! fails right away); [`CredentialVault::purge`] later destroys the
//! secret material itself, once `now >= purge_deadline`.
//!
//! Storage here is in-memory (`HashMap`) — a real deployment backs this
//! with a database, but that persistence layer is a separate concern from
//! the envelope-encryption and lifecycle logic this module owns.

use crate::kms::{Kms, KmsError};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const PURGE_GRACE_PERIOD_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Environment {
    Production,
    Test,
}

/// The raw secret pair, zeroized on drop. Never serialized, never logged.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct ApiCredential {
    pub api_key: String,
    pub api_secret: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PlaintextEnvelope {
    api_key: String,
    api_secret: String,
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("credential {0} not found")]
    NotFound(String),
    #[error("credential {0} already exists")]
    AlreadyExists(String),
    #[error("credential {0} was revoked and can no longer be retrieved")]
    Revoked(String),
    #[error("credential {0} already revoked")]
    AlreadyRevoked(String),
    #[error("credential {0} is not yet past its purge deadline")]
    PurgeNotDue(String),
    #[error("credential {0} already purged")]
    AlreadyPurged(String),
    #[error(transparent)]
    Kms(#[from] KmsError),
    #[error("encryption failed: {0}")]
    Encrypt(String),
    #[error("decryption failed: {0}")]
    Decrypt(String),
}

pub struct CredentialRecord {
    pub credential_id: String,
    pub tenant_id: String,
    pub venue_id: String,
    pub environment: Environment,
    pub created_at_ms: u64,
    pub revoked_at_ms: Option<u64>,
    pub purge_deadline_ms: Option<u64>,
    pub purged: bool,
    wrapped_dek: Vec<u8>,
    nonce: [u8; 12],
    ciphertext: Vec<u8>,
}

impl CredentialRecord {
    pub fn is_revoked(&self) -> bool {
        self.revoked_at_ms.is_some()
    }
}

pub struct CredentialVault<K: Kms> {
    kms: K,
    records: std::collections::HashMap<String, CredentialRecord>,
}

impl<K: Kms> CredentialVault<K> {
    pub fn new(kms: K) -> Self {
        Self {
            kms,
            records: std::collections::HashMap::new(),
        }
    }

    pub fn store(
        &mut self,
        credential_id: &str,
        tenant_id: &str,
        venue_id: &str,
        environment: Environment,
        credential: ApiCredential,
        now_ms: u64,
    ) -> Result<(), VaultError> {
        if self.records.contains_key(credential_id) {
            return Err(VaultError::AlreadyExists(credential_id.to_string()));
        }

        let mut dek = [0u8; 32];
        getrandom::getrandom(&mut dek).map_err(|e| VaultError::Encrypt(e.to_string()))?;
        let wrapped_dek = self.kms.wrap_dek(tenant_id, &dek)?;

        let plaintext = serde_json::to_vec(&PlaintextEnvelope {
            api_key: credential.api_key.clone(),
            api_secret: credential.api_secret.clone(),
        })
        .map_err(|e| VaultError::Encrypt(e.to_string()))?;

        let (nonce, ciphertext) = encrypt(&dek, &plaintext).map_err(VaultError::Encrypt)?;

        self.records.insert(
            credential_id.to_string(),
            CredentialRecord {
                credential_id: credential_id.to_string(),
                tenant_id: tenant_id.to_string(),
                venue_id: venue_id.to_string(),
                environment,
                created_at_ms: now_ms,
                revoked_at_ms: None,
                purge_deadline_ms: None,
                purged: false,
                wrapped_dek,
                nonce,
                ciphertext,
            },
        );
        Ok(())
    }

    /// Decrypts and returns the credential. Fails immediately once the
    /// credential has been revoked, regardless of whether the purge
    /// deadline has passed yet.
    pub fn retrieve(&self, credential_id: &str) -> Result<ApiCredential, VaultError> {
        let record = self
            .records
            .get(credential_id)
            .ok_or_else(|| VaultError::NotFound(credential_id.to_string()))?;
        if record.purged {
            return Err(VaultError::NotFound(credential_id.to_string()));
        }
        if record.is_revoked() {
            return Err(VaultError::Revoked(credential_id.to_string()));
        }

        let dek = self
            .kms
            .unwrap_dek(&record.tenant_id, &record.wrapped_dek)?;
        let mut plaintext =
            decrypt(&dek, &record.nonce, &record.ciphertext).map_err(VaultError::Decrypt)?;
        let envelope: PlaintextEnvelope =
            serde_json::from_slice(&plaintext).map_err(|e| VaultError::Decrypt(e.to_string()))?;
        plaintext.zeroize();
        Ok(ApiCredential {
            api_key: envelope.api_key,
            api_secret: envelope.api_secret,
        })
    }

    /// Immediately revokes access; sets the purge deadline 24h out.
    pub fn revoke(&mut self, credential_id: &str, now_ms: u64) -> Result<(), VaultError> {
        let record = self
            .records
            .get_mut(credential_id)
            .ok_or_else(|| VaultError::NotFound(credential_id.to_string()))?;
        if record.is_revoked() {
            return Err(VaultError::AlreadyRevoked(credential_id.to_string()));
        }
        record.revoked_at_ms = Some(now_ms);
        record.purge_deadline_ms = Some(now_ms + PURGE_GRACE_PERIOD_MS);
        Ok(())
    }

    /// Destroys the encrypted secret material. Requires the credential to
    /// be revoked and past its purge deadline.
    pub fn purge(&mut self, credential_id: &str, now_ms: u64) -> Result<(), VaultError> {
        let record = self
            .records
            .get_mut(credential_id)
            .ok_or_else(|| VaultError::NotFound(credential_id.to_string()))?;
        if record.purged {
            return Err(VaultError::AlreadyPurged(credential_id.to_string()));
        }
        let deadline = record
            .purge_deadline_ms
            .ok_or_else(|| VaultError::NotFound(format!("{credential_id} was never revoked")))?;
        if now_ms < deadline {
            return Err(VaultError::PurgeNotDue(credential_id.to_string()));
        }
        record.wrapped_dek.zeroize();
        record.ciphertext.zeroize();
        record.nonce.zeroize();
        record.purged = true;
        Ok(())
    }

    /// Credential ids that are revoked, unpurged, and past their purge
    /// deadline — a real deployment runs this on a schedule to drive
    /// `purge` calls.
    pub fn due_for_purge(&self, now_ms: u64) -> Vec<String> {
        self.records
            .values()
            .filter(|r| !r.purged && r.purge_deadline_ms.is_some_and(|d| now_ms >= d))
            .map(|r| r.credential_id.clone())
            .collect()
    }

    pub fn record(&self, credential_id: &str) -> Option<&CredentialRecord> {
        self.records.get(credential_id)
    }
}

fn encrypt(dek: &[u8; 32], plaintext: &[u8]) -> Result<([u8; 12], Vec<u8>), String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};

    let cipher = Aes256Gcm::new_from_slice(dek).map_err(|e| e.to_string())?;
    let mut nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut nonce_bytes).map_err(|e| e.to_string())?;
    let nonce = Nonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| e.to_string())?;
    Ok((nonce_bytes, ciphertext))
}

fn decrypt(dek: &[u8; 32], nonce_bytes: &[u8; 12], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};

    let cipher = Aes256Gcm::new_from_slice(dek).map_err(|e| e.to_string())?;
    let nonce = Nonce::from(*nonce_bytes);
    cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|e| e.to_string())
}
