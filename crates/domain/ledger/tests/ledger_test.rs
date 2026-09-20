//! Exercises the acceptance criteria: saldo por ativo reconcilia, conflito não
//! sobrescreve e fee é contabilizada uma vez.

use ledger::{Ledger, LedgerError, LedgerEvent, LedgerLeg};
use rust_decimal::Decimal;
use std::str::FromStr;

fn credit(namespace: &str, id: &str, asset: &str, amount: &str) -> LedgerEvent {
    LedgerEvent {
        source_namespace: namespace.to_string(),
        source_id: id.to_string(),
        economic_time_ms: 1_000,
        observed_time_ms: 1_001,
        legs: vec![LedgerLeg {
            asset: asset.to_string(),
            amount: amount.to_string(),
            is_credit: true,
        }],
        fee: None,
    }
}

#[test]
fn balance_reconciles_across_multiple_events() {
    let mut ledger = Ledger::new();
    ledger
        .ingest(credit("binance.deposit", "d1", "USDT", "100"))
        .unwrap();
    ledger
        .ingest(credit("binance.deposit", "d2", "USDT", "50"))
        .unwrap();

    let mut withdrawal = credit("binance.withdrawal", "w1", "USDT", "30");
    withdrawal.legs[0].is_credit = false;
    ledger.ingest(withdrawal).unwrap();

    assert_eq!(ledger.balance("USDT"), Decimal::from_str("120").unwrap());
}

#[test]
fn unknown_asset_has_zero_balance() {
    let ledger = Ledger::new();
    assert_eq!(ledger.balance("BTC"), Decimal::ZERO);
}

#[test]
fn trade_legs_with_two_assets_both_reconcile() {
    // A trade: debit quote asset, credit base asset.
    let mut ledger = Ledger::new();
    let event = LedgerEvent {
        source_namespace: "binance.trade".to_string(),
        source_id: "BTCUSDT#123".to_string(),
        economic_time_ms: 1_000,
        observed_time_ms: 1_001,
        legs: vec![
            LedgerLeg {
                asset: "USDT".to_string(),
                amount: "100".to_string(),
                is_credit: false,
            },
            LedgerLeg {
                asset: "BTC".to_string(),
                amount: "0.001".to_string(),
                is_credit: true,
            },
        ],
        fee: None,
    };
    ledger.ingest(event).unwrap();

    assert_eq!(ledger.balance("USDT"), Decimal::from_str("-100").unwrap());
    assert_eq!(ledger.balance("BTC"), Decimal::from_str("0.001").unwrap());
}

// --- conflict does not overwrite ---------------------------------------

#[test]
fn identical_re_ingest_is_idempotent() {
    let mut ledger = Ledger::new();
    ledger
        .ingest(credit("binance.deposit", "d1", "USDT", "100"))
        .unwrap();
    ledger
        .ingest(credit("binance.deposit", "d1", "USDT", "100"))
        .unwrap();

    assert_eq!(ledger.balance("USDT"), Decimal::from_str("100").unwrap());
    assert_eq!(ledger.event_count(), 1);
}

#[test]
fn conflicting_content_for_the_same_key_is_rejected() {
    let mut ledger = Ledger::new();
    ledger
        .ingest(credit("binance.deposit", "d1", "USDT", "100"))
        .unwrap();

    let result = ledger.ingest(credit("binance.deposit", "d1", "USDT", "999"));
    assert_eq!(
        result,
        Err(LedgerError::Conflict {
            namespace: "binance.deposit".to_string(),
            source_id: "d1".to_string()
        })
    );
}

#[test]
fn conflicting_event_does_not_change_the_balance() {
    let mut ledger = Ledger::new();
    ledger
        .ingest(credit("binance.deposit", "d1", "USDT", "100"))
        .unwrap();
    let _ = ledger.ingest(credit("binance.deposit", "d1", "USDT", "999"));

    assert_eq!(
        ledger.balance("USDT"),
        Decimal::from_str("100").unwrap(),
        "original value must be preserved"
    );
}

#[test]
fn conflicting_event_does_not_overwrite_the_stored_record() {
    let mut ledger = Ledger::new();
    ledger
        .ingest(credit("binance.deposit", "d1", "USDT", "100"))
        .unwrap();
    let _ = ledger.ingest(credit("binance.deposit", "d1", "USDT", "999"));

    let stored = ledger.event("binance.deposit", "d1").unwrap();
    assert_eq!(stored.legs[0].amount, "100");
}

#[test]
fn same_source_id_under_a_different_namespace_does_not_collide() {
    let mut ledger = Ledger::new();
    ledger
        .ingest(credit("binance.deposit", "1", "USDT", "100"))
        .unwrap();
    ledger
        .ingest(credit("binance.withdrawal", "1", "USDT", "50"))
        .unwrap();

    assert_eq!(ledger.event_count(), 2);
    assert_eq!(ledger.balance("USDT"), Decimal::from_str("150").unwrap());
}

// --- fee counted exactly once -------------------------------------------

#[test]
fn fee_is_applied_to_the_balance_exactly_once() {
    let mut ledger = Ledger::new();
    let event = LedgerEvent {
        source_namespace: "binance.withdrawal".to_string(),
        source_id: "w1".to_string(),
        economic_time_ms: 1_000,
        observed_time_ms: 1_001,
        legs: vec![LedgerLeg {
            asset: "USDT".to_string(),
            amount: "100".to_string(),
            is_credit: false,
        }],
        fee: Some(LedgerLeg {
            asset: "USDT".to_string(),
            amount: "1".to_string(),
            is_credit: false,
        }),
    };
    ledger.ingest(event).unwrap();

    // -100 principal, -1 fee: exactly -101, not -100 (fee skipped) or
    // -102 (fee double-counted).
    assert_eq!(ledger.balance("USDT"), Decimal::from_str("-101").unwrap());
}

#[test]
fn fee_in_a_different_asset_than_principal_reconciles_both() {
    // e.g. a trade whose fee is charged in BNB regardless of the traded
    // pair.
    let mut ledger = Ledger::new();
    let event = LedgerEvent {
        source_namespace: "binance.trade".to_string(),
        source_id: "BTCUSDT#1".to_string(),
        economic_time_ms: 1_000,
        observed_time_ms: 1_001,
        legs: vec![
            LedgerLeg {
                asset: "USDT".to_string(),
                amount: "100".to_string(),
                is_credit: false,
            },
            LedgerLeg {
                asset: "BTC".to_string(),
                amount: "0.001".to_string(),
                is_credit: true,
            },
        ],
        fee: Some(LedgerLeg {
            asset: "BNB".to_string(),
            amount: "0.0005".to_string(),
            is_credit: false,
        }),
    };
    ledger.ingest(event).unwrap();

    assert_eq!(ledger.balance("USDT"), Decimal::from_str("-100").unwrap());
    assert_eq!(ledger.balance("BTC"), Decimal::from_str("0.001").unwrap());
    assert_eq!(ledger.balance("BNB"), Decimal::from_str("-0.0005").unwrap());
}

// --- malformed input --------------------------------------------------

#[test]
fn invalid_amount_rejects_the_whole_event_without_partial_application() {
    let mut ledger = Ledger::new();
    let event = LedgerEvent {
        source_namespace: "binance.deposit".to_string(),
        source_id: "d1".to_string(),
        economic_time_ms: 1_000,
        observed_time_ms: 1_001,
        legs: vec![LedgerLeg {
            asset: "USDT".to_string(),
            amount: "not-a-number".to_string(),
            is_credit: true,
        }],
        fee: None,
    };
    let result = ledger.ingest(event);
    assert_eq!(
        result,
        Err(LedgerError::InvalidAmount {
            asset: "USDT".to_string(),
            amount: "not-a-number".to_string()
        })
    );
    assert_eq!(ledger.event_count(), 0);
}

#[test]
fn invalid_fee_amount_rejects_the_whole_event_even_if_legs_are_valid() {
    let mut ledger = Ledger::new();
    let event = LedgerEvent {
        source_namespace: "binance.withdrawal".to_string(),
        source_id: "w1".to_string(),
        economic_time_ms: 1_000,
        observed_time_ms: 1_001,
        legs: vec![LedgerLeg {
            asset: "USDT".to_string(),
            amount: "100".to_string(),
            is_credit: false,
        }],
        fee: Some(LedgerLeg {
            asset: "USDT".to_string(),
            amount: "garbage".to_string(),
            is_credit: false,
        }),
    };
    let result = ledger.ingest(event);
    assert!(result.is_err());
    // The principal leg must not have been applied either: all-or-nothing.
    assert_eq!(ledger.balance("USDT"), Decimal::ZERO);
}
