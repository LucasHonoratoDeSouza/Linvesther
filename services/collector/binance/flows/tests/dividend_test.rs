use binance_flows::{normalize_dividend, DividendReasonPolicy, FlowError, FlowFamily, RawDividend};

fn raw(reason: &str) -> RawDividend {
    RawDividend {
        id: "div-1".to_string(),
        asset: "BNB".to_string(),
        amount: "0.5".to_string(),
        reason: reason.to_string(),
        div_time_ms: 1_000,
    }
}

#[test]
fn homologated_reason_credits_the_account() {
    let policy = DividendReasonPolicy::new().allow("Staking");
    let flow = normalize_dividend(&raw("Staking"), &policy).unwrap();
    assert_eq!(flow.legs.len(), 1);
    assert!(flow.legs[0].is_credit);
}

#[test]
fn unrecognized_reason_blocks_valuation() {
    let policy = DividendReasonPolicy::new().allow("Staking");
    let result = normalize_dividend(&raw("MysteryBonus"), &policy);
    assert_eq!(
        result,
        Err(FlowError::UnknownKind {
            family: FlowFamily::Dividend,
            code: "MysteryBonus".to_string()
        })
    );
}

#[test]
fn empty_policy_rejects_everything() {
    let policy = DividendReasonPolicy::new();
    assert!(normalize_dividend(&raw("Staking"), &policy).is_err());
}
