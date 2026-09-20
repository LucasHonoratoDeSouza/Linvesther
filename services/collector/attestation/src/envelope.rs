//! SourceEnvelope v1, per the protocol specification: "Campos públicos:
//! `mechanism`, `issuerKeyId`, `trustManifestHash`,
//! `accountBindingCommitment`, `batchCommitment`, `rawEvidenceRoot`,
//! `normalizedRoot`, `coverageManifestHash`, `period`, `observedAt`,
//! `expiresAt`, `policyHash`, `environment`, `sourceSessionBinding`."

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEnvelope {
    pub mechanism: String,
    pub issuer_key_id: String,
    pub trust_manifest_hash: [u8; 32],
    pub account_binding_commitment: [u8; 32],
    pub batch_commitment: [u8; 32],
    pub raw_evidence_root: [u8; 32],
    pub normalized_root: [u8; 32],
    pub coverage_manifest_hash: [u8; 32],
    pub period_start_ms: i64,
    pub period_end_ms: i64,
    pub observed_at_ms: i64,
    pub expires_at_ms: i64,
    pub policy_hash: [u8; 32],
    pub environment: String,
    pub source_session_binding: [u8; 32],
}

fn write_field(buf: &mut Vec<u8>, domain: &str, bytes: &[u8]) {
    buf.extend_from_slice(domain.as_bytes());
    buf.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    buf.extend_from_slice(bytes);
}

impl SourceEnvelope {
    /// Every field, length-prefixed and domain-separated, in a fixed
    /// order. Two envelopes that differ in exactly one field — including
    /// two whose adjacent string fields could otherwise be shifted into
    /// each other — never produce the same bytes.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        write_field(&mut buf, "mechanism", self.mechanism.as_bytes());
        write_field(&mut buf, "issuerKeyId", self.issuer_key_id.as_bytes());
        write_field(&mut buf, "trustManifestHash", &self.trust_manifest_hash);
        write_field(
            &mut buf,
            "accountBindingCommitment",
            &self.account_binding_commitment,
        );
        write_field(&mut buf, "batchCommitment", &self.batch_commitment);
        write_field(&mut buf, "rawEvidenceRoot", &self.raw_evidence_root);
        write_field(&mut buf, "normalizedRoot", &self.normalized_root);
        write_field(
            &mut buf,
            "coverageManifestHash",
            &self.coverage_manifest_hash,
        );
        write_field(&mut buf, "periodStart", &self.period_start_ms.to_be_bytes());
        write_field(&mut buf, "periodEnd", &self.period_end_ms.to_be_bytes());
        write_field(&mut buf, "observedAt", &self.observed_at_ms.to_be_bytes());
        write_field(&mut buf, "expiresAt", &self.expires_at_ms.to_be_bytes());
        write_field(&mut buf, "policyHash", &self.policy_hash);
        write_field(&mut buf, "environment", self.environment.as_bytes());
        write_field(
            &mut buf,
            "sourceSessionBinding",
            &self.source_session_binding,
        );
        buf
    }

    pub fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_bytes()).into()
    }
}
