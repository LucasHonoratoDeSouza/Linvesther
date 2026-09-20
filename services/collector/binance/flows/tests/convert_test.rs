use binance_flows::{normalize_convert, FlowError, FlowFamily, RawConvert};

fn raw(status: &str) -> RawConvert {
    RawConvert {
        quote_id: "cv-1".to_string(),
        from_asset: "USDT".to_string(),
        from_amount: "100".to_string(),
        to_asset: "BTC".to_string(),
        to_amount: "0.001".to_string(),
        status: status.to_string(),
        create_time_ms: 1_000,
    }
}

#[test]
fn successful_convert_is_an_internal_swap_not_deposit_withdraw() {
    let flow = normalize_convert(&raw("SUCCESS")).unwrap();
    assert_eq!(flow.legs.len(), 2);
    assert!(!flow.legs[0].is_credit && flow.legs[0].asset == "USDT");
    assert!(flow.legs[1].is_credit && flow.legs[1].asset == "BTC");
    assert_eq!(flow.source_namespace, FlowFamily::Convert);
}

#[test]
fn processing_convert_has_no_legs_yet() {
    let flow = normalize_convert(&raw("PROCESS")).unwrap();
    assert!(flow.legs.is_empty());
}

#[test]
fn failed_convert_has_no_legs() {
    let flow = normalize_convert(&raw("FAIL")).unwrap();
    assert!(flow.legs.is_empty());
}

#[test]
fn unrecognized_status_is_rejected() {
    let result = normalize_convert(&raw("WEIRD"));
    assert_eq!(
        result,
        Err(FlowError::UnknownKind {
            family: FlowFamily::Convert,
            code: "WEIRD".to_string()
        })
    );
}
