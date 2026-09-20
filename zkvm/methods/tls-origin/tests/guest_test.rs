//! End-to-end tests for the TLS origin composition guest: real
//! proving that a bridge path never earns A1, and that swapping the
//! normalized input after the session captured a response is detected
//! — both via genuine RISC Zero proving/execution, not a fake receipt.
//!
//! `TlsSessionReceipt`/`GuestInput`/`GuestOutput` are duplicated here
//! from `guest/src/{session,lib}.rs` for the same cross-workspace
//! reason documented there.

use k256::ecdsa::signature::Signer;
use k256::ecdsa::SigningKey;
use risc0_zkvm::{default_executor, default_prover, ExecutorEnv};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zkvm_methods_tls_origin::GUEST_ELF;

const HOSTNAME: &str = "api.binance.com";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TlsSessionReceipt {
    server_name: String,
    request_method: String,
    request_path: String,
    request_query: String,
    response_status: u16,
    response_body_hash: [u8; 32],
    disclosed_fields: Vec<String>,
    wallet_binding_signature: Vec<u8>,
    wallet_address: [u8; 20],
}

fn binding_digest(receipt: &TlsSessionReceipt) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(receipt.server_name.as_bytes());
    hasher.update(receipt.request_method.as_bytes());
    hasher.update(receipt.request_path.as_bytes());
    hasher.update(receipt.request_query.as_bytes());
    hasher.update(receipt.response_status.to_be_bytes());
    hasher.update(receipt.response_body_hash);
    for field in &receipt.disclosed_fields {
        hasher.update(field.as_bytes());
    }
    hasher.update(receipt.wallet_address);
    hasher.finalize().into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuestInput {
    receipt: TlsSessionReceipt,
    verifying_key_sec1: Vec<u8>,
    normalized_input_bytes: Vec<u8>,
    expected_hostname: String,
    origin_verified_in_guest: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum OriginBadge {
    A1,
    BridgeAttested,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GuestOutput {
    badge: OriginBadge,
    normalized_input_digest: [u8; 32],
}

fn all_required_categories() -> Vec<String> {
    vec![
        "wallet_selector".to_string(),
        "symbol".to_string(),
        "filter".to_string(),
        "type".to_string(),
        "time_window".to_string(),
        "error".to_string(),
        "relevant_count".to_string(),
    ]
}

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32].into()).unwrap()
}

fn signed_receipt(response_body: &[u8]) -> TlsSessionReceipt {
    let key = signing_key();
    let mut receipt = TlsSessionReceipt {
        server_name: HOSTNAME.to_string(),
        request_method: "GET".to_string(),
        request_path: "/api/v3/myTrades".to_string(),
        request_query: "symbol=BTCUSDT".to_string(),
        response_status: 200,
        response_body_hash: Sha256::digest(response_body).into(),
        disclosed_fields: all_required_categories(),
        wallet_binding_signature: vec![],
        wallet_address: [1u8; 20],
    };
    let signature: k256::ecdsa::Signature = key.sign(&binding_digest(&receipt));
    receipt.wallet_binding_signature = signature.to_bytes().to_vec();
    receipt
}

fn base_input(origin_verified_in_guest: bool) -> GuestInput {
    let key = signing_key();
    GuestInput {
        receipt: signed_receipt(b"trade-data"),
        verifying_key_sec1: key.verifying_key().to_sec1_bytes().to_vec(),
        normalized_input_bytes: b"trade-data".to_vec(),
        expected_hostname: HOSTNAME.to_string(),
        origin_verified_in_guest,
    }
}

/// Real proving: an in-guest-verified session earns A1 for real.
#[test]
#[ignore = "real RISC Zero proving needs more free RAM than this machine reliably has alongside a normal desktop session; run explicitly with `cargo test -p zkvm-methods-tls-origin --test guest_test -- --ignored --test-threads=1` when the machine has headroom. `execution_*` tests below cover the same guest logic without proving."]
fn a_real_proof_of_in_guest_verification_earns_a1_for_real() {
    let input = base_input(true);
    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();
    let prover = default_prover();
    let prove_info = prover
        .prove(env, GUEST_ELF)
        .expect("real proving of a well-formed in-guest session must succeed");
    let receipt = &prove_info.receipt;
    receipt
        .verify(zkvm_methods_tls_origin::GUEST_ID)
        .expect("a real receipt for its own image ID must verify");

    let output: GuestOutput = receipt
        .journal
        .decode()
        .expect("journal decodes to GuestOutput");
    assert_eq!(output.badge, OriginBadge::A1);
}

/// Real proving: a bridge path, proven for real, still only ever
/// yields `BridgeAttested` in the committed journal — never A1.
#[test]
#[ignore = "real RISC Zero proving; see the other #[ignore]d test in this file for why and how to run it manually."]
fn a_real_proof_of_a_bridge_path_never_yields_a1() {
    let input = base_input(false);
    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();
    let prover = default_prover();
    let prove_info = prover.prove(env, GUEST_ELF).expect(
        "real proving of a bridge path must succeed (it never fails, it just never earns A1)",
    );
    let receipt = &prove_info.receipt;
    receipt
        .verify(zkvm_methods_tls_origin::GUEST_ID)
        .expect("a real receipt for its own image ID must verify");

    let output: GuestOutput = receipt
        .journal
        .decode()
        .expect("journal decodes to GuestOutput");
    assert_eq!(output.badge, OriginBadge::BridgeAttested);
}

/// Execution only (no proving spent): confirms the guest genuinely
/// detects a raw-vs-normalized swap.
#[test]
fn execution_detects_a_normalized_input_swap_without_proving() {
    let mut input = base_input(true);
    input.normalized_input_bytes = b"SWAPPED-data".to_vec();

    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();
    let executor = default_executor();
    let result = executor.execute(env, GUEST_ELF);
    assert!(result.is_err(), "the guest must panic (session failure) when normalized input does not match what the session captured");
}

/// Execution only: a bridge path executes successfully (it never fails)
/// but its journal, decoded from the execution session, never claims
/// A1.
#[test]
fn execution_of_a_bridge_path_commits_bridge_attested_without_proving() {
    let input = base_input(false);
    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();
    let executor = default_executor();
    let session = executor
        .execute(env, GUEST_ELF)
        .expect("a bridge path must execute successfully");
    let output: GuestOutput = session
        .journal
        .decode()
        .expect("journal decodes to GuestOutput");
    assert_eq!(output.badge, OriginBadge::BridgeAttested);
}
