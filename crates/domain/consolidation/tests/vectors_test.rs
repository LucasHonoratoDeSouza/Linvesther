//! End-to-end: exercises the acceptance criteria by feeding consolidation
//! output into `returns::compute_twr` and asserting the exact named
//! vectors from the protocol specification.

use consolidation::{
    account_added_flow, consolidated_nav, link_internal_transfer, MemberAccount, TransferLeg,
};
use returns::{compute_twr, TimelineEvent};
use rust_decimal::Decimal;
use std::collections::BTreeSet;
use std::str::FromStr;

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn index_to_return(index: Decimal) -> Decimal {
    index - Decimal::ONE
}

fn members() -> BTreeSet<String> {
    ["A".to_string(), "B".to_string()].into_iter().collect()
}

#[test]
fn transferencia_interna_sem_fee_yields_zero_return() {
    // A=100,B=0; mover 40 sem fee : NAV conjunto 100, retorno 0.
    let before = consolidated_nav(&[
        MemberAccount {
            account_id: "A".to_string(),
            nav: dec("100"),
            has_gap: false,
        },
        MemberAccount {
            account_id: "B".to_string(),
            nav: dec("0"),
            has_gap: false,
        },
    ])
    .unwrap();

    let transfer = link_internal_transfer(
        "tx-1",
        TransferLeg {
            account_id: "A".to_string(),
            asset: "USDT".to_string(),
            amount: dec("40"),
        },
        TransferLeg {
            account_id: "B".to_string(),
            asset: "USDT".to_string(),
            amount: dec("40"),
        },
        Decimal::ZERO,
        &members(),
    )
    .unwrap();
    assert_eq!(transfer.consolidated_external_flow(), Decimal::ZERO);

    let after = consolidated_nav(&[
        MemberAccount {
            account_id: "A".to_string(),
            nav: dec("60"),
            has_gap: false,
        },
        MemberAccount {
            account_id: "B".to_string(),
            nav: dec("40"),
            has_gap: false,
        },
    ])
    .unwrap();
    assert_eq!(after, dec("100"));

    // No Flow event at the consolidated level — the transfer is invisible
    // at the aggregate boundary, exactly per "conta uma vez" (once, as
    // netting, never as two external legs).
    let events = [
        TimelineEvent::Valuation {
            time_ms: 0,
            nav: before,
        },
        TimelineEvent::Valuation {
            time_ms: 1,
            nav: after,
        },
    ];
    let result = compute_twr(&events).unwrap();
    assert_eq!(index_to_return(result.index), dec("0"));
}

#[test]
fn transferencia_com_fee_yields_minus_one_percent() {
    // Mesmo caso, destino 39 e fee 1 : NAV 99, retorno -1%.
    let before = consolidated_nav(&[
        MemberAccount {
            account_id: "A".to_string(),
            nav: dec("100"),
            has_gap: false,
        },
        MemberAccount {
            account_id: "B".to_string(),
            nav: dec("0"),
            has_gap: false,
        },
    ])
    .unwrap();

    let transfer = link_internal_transfer(
        "tx-2",
        TransferLeg {
            account_id: "A".to_string(),
            asset: "USDT".to_string(),
            amount: dec("40"),
        },
        TransferLeg {
            account_id: "B".to_string(),
            asset: "USDT".to_string(),
            amount: dec("39"),
        },
        dec("1"),
        &members(),
    )
    .unwrap();
    assert_eq!(
        transfer.consolidated_external_flow(),
        Decimal::ZERO,
        "the fee is not a flow"
    );

    let after = consolidated_nav(&[
        MemberAccount {
            account_id: "A".to_string(),
            nav: dec("60"),
            has_gap: false,
        },
        MemberAccount {
            account_id: "B".to_string(),
            nav: dec("39"),
            has_gap: false,
        },
    ])
    .unwrap();
    assert_eq!(after, dec("99"));

    let events = [
        TimelineEvent::Valuation {
            time_ms: 0,
            nav: before,
        },
        TimelineEvent::Valuation {
            time_ms: 1,
            nav: after,
        },
    ];
    let result = compute_twr(&events).unwrap();
    assert_eq!(
        index_to_return(result.index),
        dec("-0.01"),
        "fee reduces NAV, producing exactly -1% with no special-cased flow bookkeeping"
    );
}

#[test]
fn conta_adicionada_yields_zero_return() {
    // A=100; entrada B=900, sem P&L : NAV 1.000, retorno 0.
    let before = consolidated_nav(&[MemberAccount {
        account_id: "A".to_string(),
        nav: dec("100"),
        has_gap: false,
    }])
    .unwrap();
    let flow = account_added_flow(dec("900"));
    let after = consolidated_nav(&[
        MemberAccount {
            account_id: "A".to_string(),
            nav: dec("100"),
            has_gap: false,
        },
        MemberAccount {
            account_id: "B".to_string(),
            nav: dec("900"),
            has_gap: false,
        },
    ])
    .unwrap();
    assert_eq!(after, dec("1000"));

    let events = [
        TimelineEvent::Valuation {
            time_ms: 0,
            nav: before,
        },
        TimelineEvent::Flow {
            time_ms: 1,
            amount: flow,
        },
        TimelineEvent::Valuation {
            time_ms: 2,
            nav: after,
        },
    ];
    let result = compute_twr(&events).unwrap();
    assert_eq!(
        index_to_return(result.index),
        dec("0"),
        "adding a member at its exact authenticated value creates no profit"
    );
}
