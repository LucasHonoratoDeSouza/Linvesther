//! apiRestrictions validation: read-only enforced, unknown fields fail
//! closed. Fixtures are synthetic JSON shaped like Binance's documented
//! response, not captured from a live account.

use collector_credentials::{validate_read_only, ApiRestrictions, RestrictionsError};

fn parse(json: &str) -> ApiRestrictions {
    serde_json::from_str(json).unwrap()
}

#[test]
fn read_only_key_passes() {
    let restrictions = parse(
        r#"{
            "ipRestrict": true,
            "enableReading": true,
            "enableWithdrawals": false,
            "enableInternalTransfer": false,
            "enableUniversalTransfer": false,
            "enableSpotAndMarginTrading": false,
            "enableMargin": false,
            "enableFutures": false,
            "permitsUniversalTransfer": false
        }"#,
    );
    assert!(validate_read_only(&restrictions).is_ok());
}

#[test]
fn a_real_binance_response_shape_passes_read_only() {
    // Captured field names (not values) from a real
    // GET /sapi/v1/account/apiRestrictions response during the live
    // homologation — the original fixture above predates these fields
    // existing on Binance's side; this confirms the now-updated struct
    // still accepts a genuinely read-only real response.
    let restrictions = parse(
        r#"{
            "ipRestrict": false,
            "createTime": 1789563842000,
            "enableReading": true,
            "enableWithdrawals": false,
            "enableInternalTransfer": false,
            "enableUniversalTransfer": false,
            "enableSpotAndMarginTrading": false,
            "enableMargin": false,
            "enableFutures": false,
            "permitsUniversalTransfer": false,
            "enableFixApiTrade": false,
            "enableFixReadOnly": false,
            "enablePortfolioMarginTrading": false,
            "enableVanillaOptions": false
        }"#,
    );
    assert!(validate_read_only(&restrictions).is_ok());
}

#[test]
fn fix_api_trade_enabled_is_rejected() {
    let restrictions = parse(
        r#"{
            "enableReading": true,
            "enableWithdrawals": false,
            "enableInternalTransfer": false,
            "enableUniversalTransfer": false,
            "enableSpotAndMarginTrading": false,
            "enableMargin": false,
            "enableFutures": false,
            "permitsUniversalTransfer": false,
            "enableFixApiTrade": true
        }"#,
    );
    assert_eq!(
        validate_read_only(&restrictions),
        Err(RestrictionsError::WriteCapabilityEnabled(
            "enableFixApiTrade"
        ))
    );
}

#[test]
fn portfolio_margin_trading_enabled_is_rejected() {
    let restrictions = parse(
        r#"{
            "enableReading": true,
            "enableWithdrawals": false,
            "enableInternalTransfer": false,
            "enableUniversalTransfer": false,
            "enableSpotAndMarginTrading": false,
            "enableMargin": false,
            "enableFutures": false,
            "permitsUniversalTransfer": false,
            "enablePortfolioMarginTrading": true
        }"#,
    );
    assert_eq!(
        validate_read_only(&restrictions),
        Err(RestrictionsError::WriteCapabilityEnabled(
            "enablePortfolioMarginTrading"
        ))
    );
}

#[test]
fn vanilla_options_enabled_is_rejected() {
    let restrictions = parse(
        r#"{
            "enableReading": true,
            "enableWithdrawals": false,
            "enableInternalTransfer": false,
            "enableUniversalTransfer": false,
            "enableSpotAndMarginTrading": false,
            "enableMargin": false,
            "enableFutures": false,
            "permitsUniversalTransfer": false,
            "enableVanillaOptions": true
        }"#,
    );
    assert_eq!(
        validate_read_only(&restrictions),
        Err(RestrictionsError::WriteCapabilityEnabled(
            "enableVanillaOptions"
        ))
    );
}

#[test]
fn fix_read_only_true_does_not_block_read_only_validation() {
    // enableFixReadOnly is a read capability, not a write one — it must
    // never be treated as disqualifying.
    let restrictions = parse(
        r#"{
            "enableReading": true,
            "enableWithdrawals": false,
            "enableInternalTransfer": false,
            "enableUniversalTransfer": false,
            "enableSpotAndMarginTrading": false,
            "enableMargin": false,
            "enableFutures": false,
            "permitsUniversalTransfer": false,
            "enableFixReadOnly": true
        }"#,
    );
    assert!(validate_read_only(&restrictions).is_ok());
}

#[test]
fn reading_disabled_is_rejected() {
    let restrictions = parse(
        r#"{
            "enableReading": false,
            "enableWithdrawals": false,
            "enableInternalTransfer": false,
            "enableUniversalTransfer": false,
            "enableSpotAndMarginTrading": false,
            "enableMargin": false,
            "enableFutures": false,
            "permitsUniversalTransfer": false
        }"#,
    );
    assert_eq!(
        validate_read_only(&restrictions),
        Err(RestrictionsError::ReadingDisabled)
    );
}

#[test]
fn withdrawals_enabled_is_rejected() {
    let restrictions = parse(
        r#"{
            "enableReading": true,
            "enableWithdrawals": true,
            "enableInternalTransfer": false,
            "enableUniversalTransfer": false,
            "enableSpotAndMarginTrading": false,
            "enableMargin": false,
            "enableFutures": false,
            "permitsUniversalTransfer": false
        }"#,
    );
    assert_eq!(
        validate_read_only(&restrictions),
        Err(RestrictionsError::WriteCapabilityEnabled(
            "enableWithdrawals"
        ))
    );
}

#[test]
fn trading_enabled_is_rejected() {
    let restrictions = parse(
        r#"{
            "enableReading": true,
            "enableWithdrawals": false,
            "enableInternalTransfer": false,
            "enableUniversalTransfer": false,
            "enableSpotAndMarginTrading": true,
            "enableMargin": false,
            "enableFutures": false,
            "permitsUniversalTransfer": false
        }"#,
    );
    assert_eq!(
        validate_read_only(&restrictions),
        Err(RestrictionsError::WriteCapabilityEnabled(
            "enableSpotAndMarginTrading"
        ))
    );
}

#[test]
fn unrecognized_field_fails_closed_even_when_every_known_field_is_read_only() {
    // Simulates Binance adding a new permission flag tomorrow that this
    // code does not yet know how to classify: must never be silently
    // ignored and treated as safe.
    let restrictions = parse(
        r#"{
            "enableReading": true,
            "enableWithdrawals": false,
            "enableInternalTransfer": false,
            "enableUniversalTransfer": false,
            "enableSpotAndMarginTrading": false,
            "enableMargin": false,
            "enableFutures": false,
            "permitsUniversalTransfer": false,
            "enableFutureProductXYZ": true
        }"#,
    );
    match validate_read_only(&restrictions) {
        Err(RestrictionsError::UnknownFields(fields)) => {
            assert_eq!(fields, vec!["enableFutureProductXYZ".to_string()]);
        }
        other => panic!("expected UnknownFields, got {other:?}"),
    }
}

#[test]
fn missing_fields_default_to_false_not_true() {
    // A minimal/older response missing several fields must not default
    // any capability to "enabled".
    let restrictions = parse(r#"{"enableReading": true}"#);
    assert!(validate_read_only(&restrictions).is_ok());
}
