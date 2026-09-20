//! A0 origin verification, per the protocol specification: "A0 assina
//! esse objeto canônico via esquema registrado... A assinatura precisa
//! cobrir os mesmos digests verificados no guest."

use crate::envelope::SourceEnvelope;
use k256::ecdsa::signature::Verifier;
use k256::ecdsa::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

/// Domain separation for [`collector_fingerprint`], so a fingerprint is
/// never confused with any other SHA-256 of the same bytes.
const FINGERPRINT_DOMAIN: &[u8] = b"LZK/collector-key/v1";

/// A collector's stable identity: SHA-256 over a domain tag and the
/// key's *compressed* SEC1 point, so the same key gives the same
/// fingerprint however it was encoded. The same function lives in
/// `services/collector/attestation` (which computes it for the worker)
/// and in `crates/verifier` (which compares it against a trust list);
/// a shared vector in each crate's tests keeps the three identical.
pub fn collector_fingerprint(verifying_key: &VerifyingKey) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(FINGERPRINT_DOMAIN);
    hasher.update(verifying_key.to_encoded_point(true).as_bytes());
    hasher.finalize().into()
}

/// What a valid origin check establishes: which envelope was signed and
/// by which collector key.
pub struct VerifiedOrigin {
    pub envelope_digest: [u8; 32],
    pub signer_fingerprint: [u8; 32],
}

#[derive(Debug)]
pub enum OriginError {
    InvalidVerifyingKey,
    InvalidSignatureEncoding,
    SignatureDoesNotVerify,
}

/// Recomputes `envelope`'s canonical digest from its own witnessed field
/// values — never trusts a precomputed digest handed in alongside it —
/// and checks `signature_compact` (a 64-byte `r || s` compact ECDSA
/// signature) against it. A host that signed one envelope cannot pass a
/// different one through under the same claimed origin: any changed
/// field changes the recomputed digest, which the signature then fails
/// to cover.
pub fn verify_origin(
    envelope: &SourceEnvelope,
    verifying_key_sec1: &[u8],
    signature_compact: &[u8],
) -> Result<VerifiedOrigin, OriginError> {
    let verifying_key = VerifyingKey::from_sec1_bytes(verifying_key_sec1)
        .map_err(|_| OriginError::InvalidVerifyingKey)?;
    let signature = Signature::from_slice(signature_compact)
        .map_err(|_| OriginError::InvalidSignatureEncoding)?;
    let digest = envelope.digest();
    verifying_key
        .verify(&digest, &signature)
        .map_err(|_| OriginError::SignatureDoesNotVerify)?;
    Ok(VerifiedOrigin {
        envelope_digest: digest,
        signer_fingerprint: collector_fingerprint(&verifying_key),
    })
}
