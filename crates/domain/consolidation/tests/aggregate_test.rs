use consolidation::{
    account_added_flow, account_removed_flow, consolidated_nav, ConsolidationError, MemberAccount,
};
use rust_decimal::Decimal;
use std::str::FromStr;

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn member(id: &str, nav: &str, has_gap: bool) -> MemberAccount {
    MemberAccount {
        account_id: id.to_string(),
        nav: dec(nav),
        has_gap,
    }
}

#[test]
fn consolidated_nav_sums_every_member() {
    let members = [member("A", "100", false), member("B", "900", false)];
    assert_eq!(consolidated_nav(&members).unwrap(), dec("1000"));
}

#[test]
fn gap_in_any_member_blocks_the_whole_aggregate() {
    let members = [member("A", "100", false), member("B", "900", true)];
    let result = consolidated_nav(&members);
    assert_eq!(
        result,
        Err(ConsolidationError::MemberGap {
            account_id: "B".to_string()
        })
    );
}

#[test]
fn gap_blocks_the_aggregate_even_if_the_gapped_member_is_small() {
    // Confirms this is not a "materiality" threshold: any gap blocks it.
    let members = [member("A", "1000000", false), member("B", "0.01", true)];
    assert!(consolidated_nav(&members).is_err());
}

#[test]
fn empty_member_set_has_zero_nav() {
    assert_eq!(consolidated_nav(&[]).unwrap(), Decimal::ZERO);
}

// --- account addition/removal: exact value, never a markup ---------------

#[test]
fn account_added_flow_equals_entry_nav_exactly() {
    // Conta adicionada: A=100; entrada B=900, sem P&L : NAV 1.000.
    assert_eq!(account_added_flow(dec("900")), dec("900"));
}

#[test]
fn account_removed_flow_is_the_negative_of_exit_nav() {
    assert_eq!(account_removed_flow(dec("250")), dec("-250"));
}
