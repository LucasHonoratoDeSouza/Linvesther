use binance_flows::{
    normalize_transfer, FlowError, FlowFamily, RawTransfer, TransferDirection, TransferTypePolicy,
};

fn raw(transfer_type: &str) -> RawTransfer {
    RawTransfer {
        tran_id: "tr-1".to_string(),
        asset: "USDT".to_string(),
        amount: "10".to_string(),
        transfer_type: transfer_type.to_string(),
        timestamp_ms: 1_000,
    }
}

#[test]
fn recognized_type_out_of_spot_debits() {
    let policy = TransferTypePolicy::new().allow("MAIN_FUNDING", TransferDirection::OutOfSpot);
    let flow = normalize_transfer(&raw("MAIN_FUNDING"), &policy).unwrap();
    assert_eq!(flow.legs.len(), 1);
    assert!(!flow.legs[0].is_credit);
}

#[test]
fn recognized_type_into_spot_credits() {
    let policy = TransferTypePolicy::new().allow("FUNDING_MAIN", TransferDirection::IntoSpot);
    let flow = normalize_transfer(&raw("FUNDING_MAIN"), &policy).unwrap();
    assert_eq!(flow.legs.len(), 1);
    assert!(flow.legs[0].is_credit);
}

#[test]
fn unrecognized_type_is_rejected() {
    let policy = TransferTypePolicy::new().allow("MAIN_FUNDING", TransferDirection::OutOfSpot);
    let result = normalize_transfer(&raw("SOME_NEW_TYPE"), &policy);
    assert_eq!(
        result,
        Err(FlowError::UnknownKind {
            family: FlowFamily::Transfer,
            code: "SOME_NEW_TYPE".to_string()
        })
    );
}

#[test]
fn empty_policy_rejects_everything() {
    let policy = TransferTypePolicy::new();
    assert!(normalize_transfer(&raw("MAIN_FUNDING"), &policy).is_err());
}
