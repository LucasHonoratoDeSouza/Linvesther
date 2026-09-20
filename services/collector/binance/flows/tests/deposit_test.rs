use binance_flows::{normalize_deposit, FlowError, FlowFamily, RawDeposit};

fn raw(status: u32) -> RawDeposit {
    RawDeposit {
        tx_id: "tx-1".to_string(),
        asset: "USDT".to_string(),
        amount: "100".to_string(),
        status,
        insert_time_ms: 1_000,
    }
}

#[test]
fn credited_deposit_produces_a_credit_leg() {
    let flow = normalize_deposit(&raw(1)).unwrap();
    assert_eq!(flow.legs.len(), 1);
    assert!(flow.legs[0].is_credit);
    assert_eq!(flow.legs[0].amount, "100");
    assert_eq!(flow.kind, "Credited");
}

#[test]
fn pending_deposit_has_no_legs() {
    let flow = normalize_deposit(&raw(0)).unwrap();
    assert!(
        flow.legs.is_empty(),
        "principal must not be recognized before it is credited"
    );
}

#[test]
fn wrong_deposit_has_no_legs() {
    let flow = normalize_deposit(&raw(7)).unwrap();
    assert!(flow.legs.is_empty());
    assert_eq!(flow.kind, "WrongDeposit");
}

#[test]
fn unrecognized_status_is_rejected() {
    let result = normalize_deposit(&raw(999));
    assert_eq!(
        result,
        Err(FlowError::UnknownKind {
            family: FlowFamily::Deposit,
            code: "999".to_string()
        })
    );
}
