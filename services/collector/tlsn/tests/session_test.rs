use collector_tlsn::{
    verify_raw_bound_to_input, verify_session_shape, verify_wallet_binding, SessionError,
    TlsSessionReceipt,
};
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

fn signed_receipt(response_body: &[u8], disclosed_fields: Vec<String>) -> TlsSessionReceipt {
    let key = signing_key();
    let mut receipt = TlsSessionReceipt {
        server_name: HOSTNAME.to_string(),
        request_method: "GET".to_string(),
        request_path: "/api/v3/myTrades".to_string(),
        request_query: "symbol=BTCUSDT".to_string(),
        response_status: 200,
        response_body_hash: Sha256::digest(response_body).into(),
        disclosed_fields,
        wallet_binding_signature: vec![],
        wallet_address: [1u8; 20],
    };
    let signature: k256::ecdsa::Signature = key.sign(&receipt.binding_digest());
    receipt.wallet_binding_signature = signature.to_bytes().to_vec();
    receipt
}

#[test]
fn a_well_formed_session_verifies_on_every_check() {
    let receipt = signed_receipt(b"trade-data", all_required_categories());
    let key = signing_key();

    assert!(verify_session_shape(&receipt, HOSTNAME).is_ok());
    assert!(verify_wallet_binding(&receipt, &key.verifying_key().to_sec1_bytes()).is_ok());
    assert!(verify_raw_bound_to_input(&receipt, b"trade-data").is_ok());
}

#[test]
fn a_hostname_other_than_binance_is_rejected() {
    let receipt = signed_receipt(b"trade-data", all_required_categories());
    let result = verify_session_shape(&receipt, HOSTNAME);
    assert!(result.is_ok());
    let wrong = verify_session_shape(&receipt, "evil.example");
    assert_eq!(
        wrong,
        Err(SessionError::HostnameMismatch {
            expected: "evil.example".to_string(),
            actual: HOSTNAME.to_string()
        })
    );
}

#[test]
fn redacting_any_required_category_is_rejected() {
    for missing in all_required_categories() {
        let fields: Vec<String> = all_required_categories()
            .into_iter()
            .filter(|f| f != &missing)
            .collect();
        let receipt = signed_receipt(b"trade-data", fields);
        let result = verify_session_shape(&receipt, HOSTNAME);
        assert!(
            result.is_err(),
            "expected redaction of {missing} to be rejected"
        );
    }
}

#[test]
fn a_tampered_receipt_field_invalidates_wallet_binding() {
    let mut receipt = signed_receipt(b"trade-data", all_required_categories());
    let key = signing_key();
    // The signature was produced over the original disclosed_fields;
    // silently un-disclosing a category after signing must invalidate
    // the binding, not just the shape check.
    receipt.disclosed_fields.pop();

    assert!(verify_wallet_binding(&receipt, &key.verifying_key().to_sec1_bytes()).is_err());
}

#[test]
fn a_signature_from_an_untrusted_key_is_rejected() {
    let receipt = signed_receipt(b"trade-data", all_required_categories());
    let other_key = SigningKey::from_bytes(&[11u8; 32].into()).unwrap();
    assert!(verify_wallet_binding(&receipt, &other_key.verifying_key().to_sec1_bytes()).is_err());
}

#[test]
fn raw_bytes_that_do_not_match_the_captured_response_are_rejected() {
    let receipt = signed_receipt(b"trade-data", all_required_categories());
    // The calculation claims to have used different bytes than what the
    // session actually captured — this must fail, not pass on a
    // best-effort basis.
    let result = verify_raw_bound_to_input(&receipt, b"substituted-data");
    assert_eq!(result, Err(SessionError::RawNotBoundToInput));
}
