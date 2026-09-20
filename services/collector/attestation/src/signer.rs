//! A0 signer, per the protocol specification: "A0 assina esse objeto
//! canônico via esquema registrado... A assinatura precisa cobrir os
//! mesmos digests verificados no guest. Trocar apenas o envelope não
//! pode trocar os dados financeiros aceitos."

use crate::envelope::SourceEnvelope;
use k256::ecdsa::signature::{Signer, Verifier};
use k256::ecdsa::{Signature, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

const FINGERPRINT_DOMAIN: &[u8] = b"LZK/collector-key/v1";

/// A collector's stable identity: SHA-256 over a domain tag and the key's
/// compressed SEC1 point. The performance guest commits the same value
/// into its journal and the verifier compares it against a trust list;
/// a shared test vector in all three places keeps them identical.
pub fn collector_fingerprint(verifying_key: &VerifyingKey) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(FINGERPRINT_DOMAIN);
    hasher.update(verifying_key.to_encoded_point(true).as_bytes());
    hasher.finalize().into()
}

#[derive(Debug, thiserror::Error)]
pub enum SignerError {
    #[error("invalid signing key bytes")]
    InvalidKey,
}

pub struct A0Signer {
    key: SigningKey,
}

impl A0Signer {
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, SignerError> {
        let key = SigningKey::from_bytes(bytes.into()).map_err(|_| SignerError::InvalidKey)?;
        Ok(Self { key })
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        *self.key.verifying_key()
    }

    /// This collector's identity, as recorded in every proof it backs.
    pub fn fingerprint(&self) -> [u8; 32] {
        collector_fingerprint(self.key.verifying_key())
    }

    /// Signs `envelope`'s canonical digest, which covers commitments
    /// (`accountBindingCommitment`, `batchCommitment`, `rawEvidenceRoot`,
    /// `normalizedRoot`, `coverageManifestHash`), policy
    /// (`policyHash`, `trustManifestHash`), binding
    /// (`sourceSessionBinding`) and the interval (`periodStart`,
    /// `periodEnd`, `observedAt`, `expiresAt`) in one pass — there is no
    /// field of the envelope this signature does not reach.
    pub fn sign(&self, envelope: &SourceEnvelope) -> Signature {
        self.key.sign(&envelope.digest())
    }
}

/// A signature verifies only against the exact envelope it was produced
/// for. Changing any single field changes `digest()`, which this
/// function checks byte-for-byte via the signature scheme — there is no
/// leniency for "close enough" envelopes.
pub fn verify(
    verifying_key: &VerifyingKey,
    envelope: &SourceEnvelope,
    signature: &Signature,
) -> bool {
    verifying_key.verify(&envelope.digest(), signature).is_ok()
}
