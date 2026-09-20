//! End-to-end tests for the performance claims guest: real proving
//! and verification of a well-formed input (with journal-tamper and
//! wrong-image-ID rejection checked against that same real receipt), and
//! fast execution-only checks that the guest genuinely refuses invalid
//! inputs (no proving spent on cases that are expected to fail).
//!
//! `SourceEnvelope`/`GuestInput`/`GuestOutput` are duplicated here from
//! `guest/src/{envelope,lib}.rs` rather than shared via a path
//! dependency: `zkvm-methods` (this package, a root-workspace member)
//! and `guest` (its own isolated workspace, riscv32im-risc0-zkvm-elf
//! target) cannot share a crate across that workspace boundary — see
//! the guest's own doc comments. The two definitions are kept in sync by
//! hand; they interoperate purely through serde's wire format, the same
//! way `experiments/risc0-benchmark`'s host and guest already do.

use k256::ecdsa::signature::Signer;
use k256::ecdsa::SigningKey;
use risc0_zkvm::{default_executor, default_prover, ExecutorEnv};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GuestOutput {
    envelope_digest: [u8; 32],
    signer_fingerprint: [u8; 32],
    period_start_ms: i64,
    period_end_ms: i64,
    twr_index_scaled: i128,
    mdd_bp: i64,
    capital: i64,
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

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32].into()).unwrap()
}

fn signed_input(envelope: SourceEnvelope, nav: Vec<i64>, ledger_deltas: Vec<i64>) -> GuestInput {
    let key = signing_key();
    let signature: k256::ecdsa::Signature = key.sign(&envelope.digest());
    GuestInput {
        envelope,
        verifying_key_sec1: key.verifying_key().to_sec1_bytes().to_vec(),
        signature_compact: signature.to_bytes().to_vec(),
        nav,
        ledger_deltas,
    }
}

/// Real proving and verification of a well-formed input, then two checks
/// against that same real receipt: a tampered journal fails verification,
/// and the receipt only verifies against its own real image ID — never
/// against an arbitrary one.
#[test]
fn a_real_proof_verifies_and_its_journal_is_tamper_evident() {
    let input = signed_input(
        base_envelope(),
        vec![100 * SCALE, 110 * SCALE],
        vec![10 * SCALE],
    );
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

    receipt
        .verify(zkvm_methods::GUEST_ID)
        .expect("a real receipt for its own image ID must verify");

    let output: GuestOutput = receipt
        .journal
        .decode()
        .expect("journal decodes to GuestOutput");
    assert_eq!(output.envelope_digest, input.envelope.digest());
    assert_eq!(
        output.signer_fingerprint.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "9c177b47de4c524a7b7754c648e488d3993b12b88930e0d67f9e3bcd01afc134",
        "the proven journal names the collector key that signed the data"
    );
    assert_eq!(output.period_start_ms, 1_000);
    assert_eq!(output.period_end_ms, 2_000);
    assert_eq!(output.twr_index_scaled, 1_100_000);
    assert_eq!(output.mdd_bp, 0);
    assert_eq!(output.capital, 110 * SCALE);

    // Tamper with the journal after the fact: the receipt's proof no
    // longer matches the (now different) claimed journal.
    let mut tampered_receipt = receipt.clone();
    let mut tampered_bytes = tampered_receipt.journal.bytes.clone();
    let last = tampered_bytes.len() - 1;
    tampered_bytes[last] ^= 0xFF;
    tampered_receipt.journal.bytes = tampered_bytes;
    assert!(
        tampered_receipt.verify(zkvm_methods::GUEST_ID).is_err(),
        "a tampered journal must not verify against the real proof"
    );

    // A real receipt for this guest must not verify against an arbitrary,
    // unrelated image ID.
    let wrong_image_id = [0u32; 8];
    assert!(
        receipt.verify(wrong_image_id).is_err(),
        "a real receipt must not verify against a mismatched image ID"
    );
}

/// Execution only (no proving spent): confirms the guest genuinely
/// refuses inputs whose A0 signature was produced over a different
/// envelope than the one submitted.
#[test]
fn execution_rejects_a_tampered_envelope_without_proving() {
    let mut input = signed_input(
        base_envelope(),
        vec![100 * SCALE, 110 * SCALE],
        vec![10 * SCALE],
    );
    input.envelope.batch_commitment = [99u8; 32];

    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();
    let executor = default_executor();
    let result = executor.execute(env, GUEST_ELF);
    assert!(
        result.is_err(),
        "the guest must panic (session failure) on a tampered envelope, never silently proceed"
    );
}

/// Execution only: confirms the guest genuinely refuses an unreconciled
/// ledger even under a valid A0 signature.
#[test]
fn execution_rejects_an_unreconciled_ledger_without_proving() {
    let input = signed_input(
        base_envelope(),
        vec![100 * SCALE, 110 * SCALE],
        vec![5 * SCALE],
    );

    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();
    let executor = default_executor();
    let result = executor.execute(env, GUEST_ELF);
    assert!(
        result.is_err(),
        "the guest must panic (session failure) when the ledger does not reconcile"
    );
}

/// Execution only: runs the real compiled guest and decodes the journal it
/// commits, without proving. Confirms the ELF that ships names the signing
/// collector and that this file's journal layout matches it.
#[test]
fn execution_commits_the_signing_collectors_fingerprint_without_proving() {
    let input = signed_input(
        base_envelope(),
        vec![100 * SCALE, 110 * SCALE],
        vec![10 * SCALE],
    );
    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();
    let session = default_executor()
        .execute(env, GUEST_ELF)
        .expect("a well-formed, correctly signed input executes");
    let output: GuestOutput = session.journal.decode().expect("journal decodes");

    assert_eq!(
        output
            .signer_fingerprint
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
        "9c177b47de4c524a7b7754c648e488d3993b12b88930e0d67f9e3bcd01afc134"
    );
    assert_eq!(output.envelope_digest, input.envelope.digest());
    assert_eq!(output.twr_index_scaled, 1_100_000);
}
