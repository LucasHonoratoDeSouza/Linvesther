//! Key-encryption-key (KEK) abstraction for envelope encryption.
//!
//! Per the operations design: "API key/secret
//! Binance só chegam ao coletor por canal autenticado e ficam em envelope
//! encryption com chave por tenant e KEK no KMS." The vault (`vault.rs`)
//! never handles a bare KEK directly; it only asks a [`Kms`] to wrap/unwrap
//! a per-credential data-encryption key (DEK), so swapping in a real cloud
//! KMS (AWS KMS, GCP KMS, ...) later means implementing this trait, not
//! rewriting the vault.

use zeroize::Zeroize;

#[derive(Debug, thiserror::Error)]
pub enum KmsError {
    #[error("KMS wrap operation failed: {0}")]
    WrapFailed(String),
    #[error("KMS unwrap operation failed: {0}")]
    UnwrapFailed(String),
}

/// Wraps and unwraps a 256-bit data-encryption key under a tenant-scoped
/// key-encryption key held by the KMS. Implementations must not persist
/// or log an unwrapped DEK; callers are responsible for zeroizing it after
/// use, which is why [`unwrap_dek`](Kms::unwrap_dek) returns a
/// [`zeroize::Zeroizing`]-wrapped value.
pub trait Kms {
    fn wrap_dek(&self, tenant_id: &str, dek: &[u8; 32]) -> Result<Vec<u8>, KmsError>;
    fn unwrap_dek(
        &self,
        tenant_id: &str,
        wrapped: &[u8],
    ) -> Result<zeroize::Zeroizing<[u8; 32]>, KmsError>;
}

/// An in-memory KEK-per-tenant KMS stand-in for local development and
/// tests. **Not a production KMS**: the KEK never leaves process memory,
/// there is no access audit trail, and losing the process loses every
/// wrapped DEK it issued. A real deployment implements [`Kms`] against an
/// actual cloud KMS; this type exists so the vault's envelope-encryption
/// logic is exercisable without one.
pub struct LocalKms {
    keks: std::collections::HashMap<String, [u8; 32]>,
}

impl LocalKms {
    /// Creates a KMS stand-in with a fresh, random KEK for `tenant_id`.
    pub fn with_random_kek(tenant_id: impl Into<String>) -> Result<Self, KmsError> {
        let mut kek = [0u8; 32];
        getrandom::getrandom(&mut kek).map_err(|e| KmsError::WrapFailed(e.to_string()))?;
        let mut keks = std::collections::HashMap::new();
        keks.insert(tenant_id.into(), kek);
        Ok(Self { keks })
    }
}

impl Kms for LocalKms {
    fn wrap_dek(&self, tenant_id: &str, dek: &[u8; 32]) -> Result<Vec<u8>, KmsError> {
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        let kek = self
            .keks
            .get(tenant_id)
            .ok_or_else(|| KmsError::WrapFailed(format!("unknown tenant: {tenant_id}")))?;
        let cipher =
            Aes256Gcm::new_from_slice(kek).map_err(|e| KmsError::WrapFailed(e.to_string()))?;

        let mut nonce_bytes = [0u8; 12];
        getrandom::getrandom(&mut nonce_bytes).map_err(|e| KmsError::WrapFailed(e.to_string()))?;
        let nonce = Nonce::from(nonce_bytes);

        let ciphertext = cipher
            .encrypt(&nonce, dek.as_slice())
            .map_err(|e| KmsError::WrapFailed(e.to_string()))?;

        let mut wrapped = Vec::with_capacity(12 + ciphertext.len());
        wrapped.extend_from_slice(&nonce_bytes);
        wrapped.extend_from_slice(&ciphertext);
        Ok(wrapped)
    }

    fn unwrap_dek(
        &self,
        tenant_id: &str,
        wrapped: &[u8],
    ) -> Result<zeroize::Zeroizing<[u8; 32]>, KmsError> {
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        if wrapped.len() < 12 {
            return Err(KmsError::UnwrapFailed(
                "wrapped DEK shorter than nonce".into(),
            ));
        }
        let kek = self
            .keks
            .get(tenant_id)
            .ok_or_else(|| KmsError::UnwrapFailed(format!("unknown tenant: {tenant_id}")))?;
        let cipher =
            Aes256Gcm::new_from_slice(kek).map_err(|e| KmsError::UnwrapFailed(e.to_string()))?;

        let (nonce_bytes, ciphertext) = wrapped.split_at(12);
        let nonce_array: [u8; 12] = nonce_bytes
            .try_into()
            .expect("split_at(12) guarantees a 12-byte slice");
        let nonce = Nonce::from(nonce_array);
        let mut plaintext = cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|e| KmsError::UnwrapFailed(e.to_string()))?;

        if plaintext.len() != 32 {
            plaintext.zeroize();
            return Err(KmsError::UnwrapFailed(
                "unwrapped DEK is not 32 bytes".into(),
            ));
        }
        let mut dek = [0u8; 32];
        dek.copy_from_slice(&plaintext);
        plaintext.zeroize();
        Ok(zeroize::Zeroizing::new(dek))
    }
}
