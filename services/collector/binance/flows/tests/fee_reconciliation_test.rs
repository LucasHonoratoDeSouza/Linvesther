//! Cross-cutting: a fee is represented exactly once (its own field),
//! never folded into or duplicated across the principal legs, for every
//! family that charges one.

use binance_flows::{normalize_dust, normalize_withdrawal, RawDust, RawDustDetail, RawWithdrawal};

#[test]
fn withdrawal_fee_appears_once_not_inside_the_principal_amount() {
    let raw = RawWithdrawal {
        id: "wd-1".to_string(),
        asset: "USDT".to_string(),
        amount: "100".to_string(),
        transaction_fee: "2".to_string(),
        status: 6,
        apply_time_ms: 1_000,
    };
    let flow = normalize_withdrawal(&raw).unwrap();

    // Principal leg is exactly the requested amount, unmodified by fee.
    assert_eq!(flow.legs[0].amount, "100");
    // Fee is a distinct leg.
    let fee = flow.fee.as_ref().unwrap();
    assert_eq!(fee.amount, "2");
    // The fee leg is not present a second time among the principal legs.
    assert_eq!(flow.legs.len(), 1);
}

#[test]
fn dust_service_charge_appears_once_not_inside_transferred_amount() {
    let raw = RawDust {
        dribblet_id: "dust-1".to_string(),
        to_asset: "BNB".to_string(),
        total_transferred: "0.01".to_string(),
        total_service_charge: "0.0002".to_string(),
        details: vec![RawDustDetail {
            from_asset: "DOGE".to_string(),
            amount: "5".to_string(),
        }],
        operate_time_ms: 1_000,
    };
    let flow = normalize_dust(&raw).unwrap();

    let credit_leg = flow.legs.iter().find(|l| l.is_credit).unwrap();
    assert_eq!(
        credit_leg.amount, "0.01",
        "credited amount excludes the fee, not netted"
    );
    let fee = flow.fee.as_ref().unwrap();
    assert_eq!(fee.amount, "0.0002");
}
