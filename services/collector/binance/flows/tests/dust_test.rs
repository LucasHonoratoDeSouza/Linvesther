use binance_flows::{normalize_dust, FlowError, RawDust, RawDustDetail};

#[test]
fn dust_conversion_debits_every_source_asset_and_credits_destination() {
    let raw = RawDust {
        dribblet_id: "dust-1".to_string(),
        to_asset: "BNB".to_string(),
        total_transferred: "0.01".to_string(),
        total_service_charge: "0.0001".to_string(),
        details: vec![
            RawDustDetail {
                from_asset: "DOGE".to_string(),
                amount: "5".to_string(),
            },
            RawDustDetail {
                from_asset: "TRX".to_string(),
                amount: "3".to_string(),
            },
        ],
        operate_time_ms: 1_000,
    };

    let flow = normalize_dust(&raw).unwrap();
    assert_eq!(flow.legs.len(), 3); // 2 debits + 1 credit
    assert!(!flow.legs[0].is_credit && flow.legs[0].asset == "DOGE");
    assert!(!flow.legs[1].is_credit && flow.legs[1].asset == "TRX");
    assert!(flow.legs[2].is_credit && flow.legs[2].asset == "BNB");

    let fee = flow.fee.unwrap();
    assert!(!fee.is_credit);
    assert_eq!(fee.asset, "BNB");
    assert_eq!(fee.amount, "0.0001");
}

#[test]
fn dust_with_no_details_is_rejected() {
    let raw = RawDust {
        dribblet_id: "dust-2".to_string(),
        to_asset: "BNB".to_string(),
        total_transferred: "0".to_string(),
        total_service_charge: "0".to_string(),
        details: vec![],
        operate_time_ms: 1_000,
    };
    assert!(matches!(
        normalize_dust(&raw),
        Err(FlowError::UnknownKind { .. })
    ));
}

#[test]
fn zero_service_charge_is_not_recorded_as_a_fee_leg() {
    let raw = RawDust {
        dribblet_id: "dust-3".to_string(),
        to_asset: "BNB".to_string(),
        total_transferred: "0.01".to_string(),
        total_service_charge: "0".to_string(),
        details: vec![RawDustDetail {
            from_asset: "DOGE".to_string(),
            amount: "5".to_string(),
        }],
        operate_time_ms: 1_000,
    };
    let flow = normalize_dust(&raw).unwrap();
    assert!(flow.fee.is_none());
}
