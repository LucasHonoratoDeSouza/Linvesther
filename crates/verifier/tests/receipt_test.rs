//! Real receipt verification ("image" failure mode), against a real
//! RISC Zero proof produced by `zkvm-methods` — not a fake/dev-mode
//! receipt. One real proving pass; every assertion below (success,
//! wrong image ID, tampered journal) reuses that same receipt, matching
//! the pattern in `zkvm/methods/performance/tests/guest_test.rs`.

use k256::ecdsa::signature::Signer;
use k256::ecdsa::SigningKey;
use risc0_zkvm::{default_prover, ExecutorEnv};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use verifier::verify_receipt;
use zkvm_methods::GUEST_ELF;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceEnvelope {
    mechanism: String,
    issuer_key_id: String,
    trust_manifest_hash: [u8; 32],
    account_binding_commitment: [u8; 32],
    batch_commitment: [u8; 32],
    raw_evidence_root: [u8; 32],
    normalized_root: [u8; 32],
    coverage_manifest_hash: [u8; 32],
    period_start_ms: i64,
    period_end_ms: i64,
    observed_at_ms: i64,
    expires_at_ms: i64,
    policy_hash: [u8; 32],
    environment: String,
    source_session_binding: [u8; 32],
}

fn write_field(buf: &mut Vec<u8>, domain: &str, bytes: &[u8]) {
    buf.extend_from_slice(domain.as_bytes());
    buf.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    buf.extend_from_slice(bytes);
}

impl SourceEnvelope {
    fn canonical_bytes(&self) -> Vec<u8> {
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

    fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_bytes()).into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuestInput {
    envelope: SourceEnvelope,
    verifying_key_sec1: Vec<u8>,
    signature_compact: Vec<u8>,
    nav: Vec<i64>,
    ledger_deltas: Vec<i64>,
}

const SCALE: i64 = 1_000_000;

fn base_envelope() -> SourceEnvelope {
    SourceEnvelope {
        mechanism: "A0".to_string(),
        issuer_key_id: "collector-1".to_string(),
        trust_manifest_hash: [1u8; 32],
        account_binding_commitment: [2u8; 32],
        batch_commitment: [3u8; 32],
        raw_evidence_root: [4u8; 32],
        normalized_root: [5u8; 32],
        coverage_manifest_hash: [6u8; 32],
        period_start_ms: 1_000,
        period_end_ms: 2_000,
        observed_at_ms: 1_500,
        expires_at_ms: 100_000,
        policy_hash: [8u8; 32],
        environment: "production".to_string(),
        source_session_binding: [9u8; 32],
    }
}

#[test]
#[ignore = "real RISC Zero proving needs more free RAM than this machine reliably has alongside a normal desktop session (~15min, observed to push the host to <1GB available and freeze once already); run explicitly with `cargo test -p verifier --test receipt_test -- --ignored --test-threads=1` when the machine has headroom (close other heavy apps first), or on a machine/CI runner with more memory to spare. the performance guest's own guest_test.rs already proves this exact guest for real; this test additionally proves that `verifier::verify_receipt` correctly wraps that same verification and its journal-equality check."]
fn a_real_receipt_verifies_a_tampered_journal_fails_and_a_wrong_image_id_fails() {
    let key = SigningKey::from_bytes(&[7u8; 32].into()).unwrap();
    let envelope = base_envelope();
    let signature: k256::ecdsa::Signature = key.sign(&envelope.digest());
    let input = GuestInput {
        envelope,
        verifying_key_sec1: key.verifying_key().to_sec1_bytes().to_vec(),
        signature_compact: signature.to_bytes().to_vec(),
        nav: vec![100 * SCALE, 110 * SCALE],
        ledger_deltas: vec![10 * SCALE],
    };

    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();
    let prover = default_prover();
    let prove_info = prover
        .prove(env, GUEST_ELF)
        .expect("real proving of a well-formed input must succeed");
    let receipt = &prove_info.receipt;
    let receipt_bytes = bincode::serialize(receipt).expect("receipt must serialize");
    let real_journal = receipt.journal.bytes.clone();

    // Success: real receipt, real image ID, matching journal.
    assert!(verify_receipt(&receipt_bytes, zkvm_methods::GUEST_ID, &real_journal).is_ok());

    // "image" failure mode: wrong image ID is rejected.
    let wrong_image_id = [0u32; 8];
    assert!(verify_receipt(&receipt_bytes, wrong_image_id, &real_journal).is_err());

    // Journal swapped after proving is rejected, even against the
    // correct image ID and a structurally valid real receipt.
    let mut tampered_journal = real_journal.clone();
    let last = tampered_journal.len() - 1;
    tampered_journal[last] ^= 0xFF;
    assert!(verify_receipt(&receipt_bytes, zkvm_methods::GUEST_ID, &tampered_journal).is_err());
}

/// The same bytes are produced by the guest's own test
/// (`zkvm/methods/performance/guest/tests/run_test.rs`). If the guest's
/// journal layout changes, that test and this one fail together, so this
/// decoder cannot silently drift from what is actually proven.
const GOLDEN_JOURNAL_HEX: &str = "760000009300000042000000c2000000a6000000690000002600000077000000810000002d000000f700000013000000030000005b0000002500000093000000fb000000f2000000d60000007b000000150000004e000000b400000080000000f200000083000000500000000a0000001c00000056000000a70000002f0000009c000000170000007b00000047000000de0000004c000000520000004a0000007b0000007700000054000000c600000048000000e400000088000000d3000000990000003b00000012000000b80000008900000030000000e0000000d60000007f0000009e0000003b000000cd00000001000000af000000c100000034000000e803000000000000d007000000000000e0c81000000000000000000000000000000000000000000080778e0600000000";

#[test]
fn the_journal_decodes_with_the_signing_collector_named() {
    let bytes = hex::decode(GOLDEN_JOURNAL_HEX).unwrap();
    let journal = verifier::decode_journal_bytes(&bytes).expect("golden journal decodes");
    assert_eq!(
        hex::encode(journal.signer_fingerprint),
        "9c177b47de4c524a7b7754c648e488d3993b12b88930e0d67f9e3bcd01afc134"
    );
    assert_eq!(journal.period_start_ms, 1_000);
    assert_eq!(journal.period_end_ms, 2_000);
    assert_eq!(journal.twr_index_scaled, 1_100_000);
    assert_eq!(journal.mdd_bp, 0);
    assert_eq!(journal.capital, 110_000_000);
}

#[test]
fn a_truncated_journal_is_refused() {
    let bytes = hex::decode(GOLDEN_JOURNAL_HEX).unwrap();
    assert!(verifier::decode_journal_bytes(&bytes[..bytes.len() - 8]).is_err());
}
