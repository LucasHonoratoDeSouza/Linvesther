//! Exercises the acceptance criteria: NAV usa livre+bloqueado uma vez,
//! reconstrói corte e recusa ordem temporal ambígua ou ativo sem suporte.

use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::str::FromStr;
use valuation::{value_at_cut, AssetInput, BalanceSnapshot, TimedDelta, ValuationError};

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

#[test]
fn nav_sums_quantity_times_price_across_assets() {
    let inputs = vec![
        AssetInput {
            asset: "BTC".to_string(),
            snapshot: BalanceSnapshot {
                free: dec("1"),
                locked: dec("0"),
                observed_at_ms: 1_000,
            },
            events: &[],
        },
        AssetInput {
            asset: "USDT".to_string(),
            snapshot: BalanceSnapshot {
                free: dec("5000"),
                locked: dec("0"),
                observed_at_ms: 1_000,
            },
            events: &[],
        },
    ];
    let mut prices = BTreeMap::new();
    prices.insert("BTC".to_string(), dec("50000"));
    prices.insert("USDT".to_string(), dec("1"));

    let valuation = value_at_cut(&inputs, &prices, 1_000).unwrap();
    assert_eq!(valuation.nav, dec("55000"));
}

#[test]
fn nav_counts_free_and_locked_once_each() {
    let inputs = vec![AssetInput {
        asset: "BTC".to_string(),
        snapshot: BalanceSnapshot {
            free: dec("1"),
            locked: dec("0.5"),
            observed_at_ms: 1_000,
        },
        events: &[],
    }];
    let mut prices = BTreeMap::new();
    prices.insert("BTC".to_string(), dec("100"));

    let valuation = value_at_cut(&inputs, &prices, 1_000).unwrap();
    // 1.5 BTC total (free+locked, once) * 100 = 150, not 200 (double-count
    // of locked) or 100 (locked ignored).
    assert_eq!(valuation.nav, dec("150"));
}

#[test]
fn valuation_reconstructs_holdings_at_the_common_cut() {
    let events = [TimedDelta {
        time_ms: 1_500,
        delta: dec("50"),
    }];
    let inputs = vec![AssetInput {
        asset: "USDT".to_string(),
        snapshot: BalanceSnapshot {
            free: dec("100"),
            locked: dec("0"),
            observed_at_ms: 1_000,
        },
        events: &events,
    }];
    let mut prices = BTreeMap::new();
    prices.insert("USDT".to_string(), dec("1"));

    let valuation = value_at_cut(&inputs, &prices, 2_000).unwrap();
    assert_eq!(
        valuation.nav,
        dec("150"),
        "the +50 deposit between snapshot and cut must be included"
    );
}

#[test]
fn ambiguous_ordering_refuses_the_entire_valuation() {
    let btc_events = [TimedDelta {
        time_ms: 1_000,
        delta: dec("1"),
    }]; // tie: ambiguous
    let inputs = vec![
        AssetInput {
            asset: "BTC".to_string(),
            snapshot: BalanceSnapshot {
                free: dec("1"),
                locked: dec("0"),
                observed_at_ms: 1_000,
            },
            events: &btc_events,
        },
        AssetInput {
            asset: "USDT".to_string(),
            snapshot: BalanceSnapshot {
                free: dec("5000"),
                locked: dec("0"),
                observed_at_ms: 1_000,
            },
            events: &[],
        },
    ];
    let mut prices = BTreeMap::new();
    prices.insert("BTC".to_string(), dec("50000"));
    prices.insert("USDT".to_string(), dec("1"));

    let result = value_at_cut(&inputs, &prices, 2_000);
    assert!(
        matches!(result, Err(ValuationError::AmbiguousOrdering { asset, .. }) if asset == "BTC")
    );
}

#[test]
fn asset_without_price_support_refuses_the_entire_valuation() {
    let inputs = vec![
        AssetInput {
            asset: "BTC".to_string(),
            snapshot: BalanceSnapshot {
                free: dec("1"),
                locked: dec("0"),
                observed_at_ms: 1_000,
            },
            events: &[],
        },
        AssetInput {
            asset: "SOMEUNSUPPORTEDCOIN".to_string(),
            snapshot: BalanceSnapshot {
                free: dec("100"),
                locked: dec("0"),
                observed_at_ms: 1_000,
            },
            events: &[],
        },
    ];
    let mut prices = BTreeMap::new();
    prices.insert("BTC".to_string(), dec("50000")); // SOMEUNSUPPORTEDCOIN has no entry

    let result = value_at_cut(&inputs, &prices, 1_000);
    assert_eq!(
        result,
        Err(ValuationError::UnsupportedAsset(
            "SOMEUNSUPPORTEDCOIN".to_string()
        ))
    );
}

#[test]
fn a_single_unsupported_asset_blocks_the_whole_nav_not_just_its_own_contribution() {
    // Confirms the refusal is total, not a partial NAV silently omitting
    // the unsupported asset's value.
    let inputs = vec![
        AssetInput {
            asset: "BTC".to_string(),
            snapshot: BalanceSnapshot {
                free: dec("1"),
                locked: dec("0"),
                observed_at_ms: 1_000,
            },
            events: &[],
        },
        AssetInput {
            asset: "NOPRICE".to_string(),
            snapshot: BalanceSnapshot {
                free: dec("1000000"),
                locked: dec("0"),
                observed_at_ms: 1_000,
            },
            events: &[],
        },
    ];
    let mut prices = BTreeMap::new();
    prices.insert("BTC".to_string(), dec("50000"));

    // If this silently produced Ok(nav=50000) that would be exactly the
    // bug this test guards against: the valuation must be Err, full stop.
    assert!(value_at_cut(&inputs, &prices, 1_000).is_err());
}

#[test]
fn empty_inputs_produce_zero_nav() {
    let prices = BTreeMap::new();
    let valuation = value_at_cut(&[], &prices, 1_000).unwrap();
    assert_eq!(valuation.nav, Decimal::ZERO);
}
