use binance_flows::{normalize_withdrawal, FlowError, FlowFamily, RawWithdrawal};

fn raw(status: u32) -> RawWithdrawal {
    RawWithdrawal {
        id: "wd-1".to_string(),
        asset: "USDT".to_string(),
        amount: "50".to_string(),
        transaction_fee: "1".to_string(),
        status,
        apply_time_ms: 1_000,
    }
}

#[test]
fn completed_withdrawal_debits_principal_and_fee_separately() {
    let flow = normalize_withdrawal(&raw(6)).unwrap();
    assert_eq!(flow.legs.len(), 1);
    assert!(!flow.legs[0].is_credit);
    assert_eq!(flow.legs[0].amount, "50");
    let fee = flow.fee.unwrap();
    assert!(!fee.is_credit);
    assert_eq!(fee.amount, "1");
    // fee is not folded into the principal leg's amount.
    assert_eq!(flow.legs[0].amount, "50");
}

#[test]
fn requested_withdrawal_has_no_legs_yet() {
    let flow = normalize_withdrawal(&raw(0)).unwrap();
    assert!(flow.legs.is_empty());
    assert_eq!(flow.kind, "Requested");
}

#[test]
fn cancelled_withdrawal_is_reversed_never_debited() {
    let flow = normalize_withdrawal(&raw(1)).unwrap();
    assert!(flow.legs.is_empty());
    assert_eq!(flow.kind, "Reversed");
}

#[test]
fn failed_withdrawal_never_debits() {
    let flow = normalize_withdrawal(&raw(5)).unwrap();
    assert!(flow.legs.is_empty());
    assert_eq!(flow.kind, "Failed");
}

#[test]
fn unrecognized_status_is_rejected() {
    let result = normalize_withdrawal(&raw(42));
    assert_eq!(
        result,
        Err(FlowError::UnknownKind {
            family: FlowFamily::Withdrawal,
            code: "42".to_string()
        })
    );
}

#[test]
fn zero_fee_is_not_recorded_as_a_fee_leg() {
    let mut r = raw(6);
    r.transaction_fee = "0".to_string();
    let flow = normalize_withdrawal(&r).unwrap();
    assert!(flow.fee.is_none());
}
