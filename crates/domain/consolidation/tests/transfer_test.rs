use consolidation::{link_internal_transfer, ConsolidationError, TransferLeg};
use rust_decimal::Decimal;
use std::collections::BTreeSet;
use std::str::FromStr;

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn members() -> BTreeSet<String> {
    ["A".to_string(), "B".to_string()].into_iter().collect()
}

fn leg(account: &str, asset: &str, amount: &str) -> TransferLeg {
    TransferLeg {
        account_id: account.to_string(),
        asset: asset.to_string(),
        amount: dec(amount),
    }
}

#[test]
fn linked_transfer_without_fee_reconciles() {
    let transfer = link_internal_transfer(
        "tx-1",
        leg("A", "USDT", "40"),
        leg("B", "USDT", "40"),
        Decimal::ZERO,
        &members(),
    )
    .unwrap();
    assert_eq!(transfer.fee, Decimal::ZERO);
    assert_eq!(
        transfer.consolidated_external_flow(),
        Decimal::ZERO,
        "an internal transfer is never an external flow"
    );
}

#[test]
fn linked_transfer_with_fee_reconciles_outgoing_as_incoming_plus_fee() {
    let transfer = link_internal_transfer(
        "tx-2",
        leg("A", "USDT", "40"),
        leg("B", "USDT", "39"),
        dec("1"),
        &members(),
    )
    .unwrap();
    assert_eq!(transfer.fee, dec("1"));
    assert_eq!(
        transfer.consolidated_external_flow(),
        Decimal::ZERO,
        "the fee reduces NAV naturally, it is not modeled as a flow"
    );
}

#[test]
fn unreconciled_amounts_are_rejected() {
    // outgoing=40, incoming=35, claimed fee=1: 35+1 != 40.
    let result = link_internal_transfer(
        "tx-3",
        leg("A", "USDT", "40"),
        leg("B", "USDT", "35"),
        dec("1"),
        &members(),
    );
    assert_eq!(
        result,
        Err(ConsolidationError::UnreconciledAmounts {
            outgoing: dec("40"),
            incoming_plus_fee: dec("36")
        })
    );
}

#[test]
fn matching_value_and_timing_alone_is_not_a_link() {
    // Different link ids conceptually represent unrelated events; this
    // crate never infers a link from amounts matching by coincidence —
    // callers must supply a link_id from source/tx/network evidence. This
    // test documents that link_internal_transfer always requires an
    // explicit link_id parameter (there is no "infer from amounts"
    // constructor at all).
    let transfer = link_internal_transfer(
        "tx-4",
        leg("A", "USDT", "40"),
        leg("B", "USDT", "40"),
        Decimal::ZERO,
        &members(),
    )
    .unwrap();
    assert_eq!(transfer.link_id, "tx-4");
}

#[test]
fn outgoing_account_not_a_member_is_rejected() {
    let result = link_internal_transfer(
        "tx-5",
        leg("C", "USDT", "40"),
        leg("B", "USDT", "40"),
        Decimal::ZERO,
        &members(),
    );
    assert!(matches!(result, Err(ConsolidationError::NotAMember { .. })));
}

#[test]
fn incoming_account_not_a_member_is_rejected() {
    let result = link_internal_transfer(
        "tx-6",
        leg("A", "USDT", "40"),
        leg("D", "USDT", "40"),
        Decimal::ZERO,
        &members(),
    );
    assert!(matches!(result, Err(ConsolidationError::NotAMember { .. })));
}

#[test]
fn same_account_on_both_sides_is_rejected() {
    let result = link_internal_transfer(
        "tx-7",
        leg("A", "USDT", "40"),
        leg("A", "USDT", "40"),
        Decimal::ZERO,
        &members(),
    );
    assert_eq!(
        result,
        Err(ConsolidationError::SameAccount {
            account_id: "A".to_string()
        })
    );
}

#[test]
fn asset_mismatch_is_rejected() {
    let result = link_internal_transfer(
        "tx-8",
        leg("A", "USDT", "40"),
        leg("B", "BTC", "0.001"),
        Decimal::ZERO,
        &members(),
    );
    assert!(matches!(
        result,
        Err(ConsolidationError::AssetMismatch { .. })
    ));
}
