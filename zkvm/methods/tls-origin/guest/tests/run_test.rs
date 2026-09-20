use guest::session::TlsSessionReceipt;
use guest::{run, GuestError, GuestInput, OriginBadge};
use k256::ecdsa::signature::Signer;
use k256::ecdsa::SigningKey;
use sha2::{Digest, Sha256};

const HOSTNAME: &str = "api.binance.com";

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
    let signature: k256::ecdsa::Signature = key.sign(&receipt.binding_digest());
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

#[test]
fn in_guest_verification_of_a_well_formed_session_earns_a1() {
    let input = base_input(true);
    let output = run(&input).expect("well-formed in-guest session must succeed");
    assert_eq!(output.badge, OriginBadge::A1);
    let expected_digest: [u8; 32] = Sha256::digest(b"trade-data").into();
    assert_eq!(output.normalized_input_digest, expected_digest);
}

#[test]
fn a_bridge_path_never_earns_a1_even_with_a_perfectly_valid_receipt() {
    // Same receipt, same everything — only origin_verified_in_guest
    // flips to false. There is no way to reach A1 from this path.
    let input = base_input(false);
    let output = run(&input).expect("bridge path never fails, it just never earns A1");
    assert_eq!(output.badge, OriginBadge::BridgeAttested);
}

#[test]
fn swapping_the_normalized_input_after_the_session_captured_a_response_is_detected() {
    let mut input = base_input(true);
    input.normalized_input_bytes = b"SWAPPED-data".to_vec();
    let result = run(&input);
    assert!(matches!(result, Err(GuestError::Session(_))));
}

#[test]
fn redacting_a_required_category_blocks_a1() {
    let mut input = base_input(true);
    input.receipt.disclosed_fields.pop();
    // Note: this also breaks the wallet-binding signature (it covered
    // the original disclosed_fields), so either check could fire first;
    // the guest must fail either way, never emit a badge.
    let result = run(&input);
    assert!(result.is_err());
}

#[test]
fn a_signature_from_an_untrusted_key_blocks_a1() {
    let mut input = base_input(true);
    let other_key = SigningKey::from_bytes(&[11u8; 32].into()).unwrap();
    input.verifying_key_sec1 = other_key.verifying_key().to_sec1_bytes().to_vec();
    let result = run(&input);
    assert!(matches!(result, Err(GuestError::Session(_))));
}
