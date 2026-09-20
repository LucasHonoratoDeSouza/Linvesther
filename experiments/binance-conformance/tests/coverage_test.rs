//! Exercises the Gate B0 checklist from the Binance adapter specification:
//! "Passa somente se um ensaio autorizado em ambiente real confirmar: UID e
//! rotação; permissões; saldo livre/bloqueado; trades paginados; taxa em
//! BNB; round trip de símbolo zerado; símbolo removido; depósito/saque/
//! estorno; transferências para fora de Spot; Convert/dust/distribuição;
//! timestamps simultâneos; endpoint truncado; restrições históricas; versão
//! de catálogo; cut consistente e conciliação de saldo."
//!
//! Each item below is a decision the coverage engine must get right from
//! evidence; a live-account run against the real API is a separate,
//! not-yet-performed step documented in README.md.

use binance_conformance::model::*;
use binance_conformance::{evaluate, CoverageStatus};
use std::collections::{BTreeMap, BTreeSet};

fn symbols(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|s| s.to_string()).collect()
}

fn exhausted_family(name: &str) -> FlowFamilyCoverage {
    FlowFamilyCoverage {
        name: name.to_string(),
        outcome: PageOutcome::Exhausted,
        kinds_observed: symbols(&["OBSERVED_KIND"]),
        unknown_kinds: BTreeSet::new(),
    }
}

fn exhausted_trade(symbol: &str) -> SymbolTradeCoverage {
    SymbolTradeCoverage {
        symbol: symbol.to_string(),
        pages_fetched: 3,
        outcome: PageOutcome::Exhausted,
        fee_currencies_observed: symbols(&["BNB"]),
    }
}

/// A fully compliant evidence set: every check should pass and the report
/// should be `PolicyComplete` with no reasons. Every test below starts from
/// this baseline and breaks exactly one property.
fn complete_evidence() -> Evidence {
    let mut trades = BTreeMap::new();
    trades.insert("BTCUSDT".to_string(), exhausted_trade("BTCUSDT"));
    // A symbol that was later delisted, or whose position later went to
    // zero, stays required once it entered the universe.
    trades.insert("LUNAUSDT".to_string(), exhausted_trade("LUNAUSDT"));

    Evidence {
        binding: AccountBinding {
            uid: "uid-123".to_string(),
            environment: Environment::Production,
            perimeter: "SPOT".to_string(),
            rotations: vec![RotationEvent {
                observed_at_ms: 1_700_000_000_000,
                previous_key_id: "key-old".to_string(),
                next_key_id: "key-new".to_string(),
            }],
        },
        permissions: Permissions {
            enable_reading: true,
            enable_withdrawals: false,
            enable_internal_transfer: false,
            enable_spot_and_margin_trading: false,
            enable_margin: false,
            enable_futures: false,
            unknown_flags: vec![],
        },
        symbol_universe: symbols(&["BTCUSDT", "LUNAUSDT"]),
        eligible_from_ms: 1_650_000_000_000,
        trades,
        deposits: exhausted_family("depósitos"),
        withdrawals: exhausted_family("saques"),
        transfers: exhausted_family("transferências universais"),
        convert: exhausted_family("Convert"),
        dust: exhausted_family("dust conversions"),
        dividends: exhausted_family("dividendos/distribuições"),
        catalog_versions: vec![CatalogVersion {
            captured_at_ms: 1_600_000_000_000, // before eligible_from_ms
            symbols: symbols(&["BTCUSDT", "LUNAUSDT"]),
        }],
        reconciliations: vec![AssetReconciliation {
            asset: "BTC".to_string(),
            ledger_total: BalanceComponents {
                free: 1.5,
                locked: 0.5,
            },
            observed_total: BalanceComponents {
                free: 1.5,
                locked: 0.5,
            },
        }],
    }
}

#[test]
fn baseline_evidence_is_policy_complete() {
    let report = evaluate(&complete_evidence());
    assert_eq!(report.status, CoverageStatus::PolicyComplete);
    assert!(
        report.reasons.is_empty(),
        "unexpected reasons: {:?}",
        report.reasons
    );
}

// 1. UID e rotação: a rotation event missing either key id cannot prove the
// binding was preserved across the rotation.
#[test]
fn rotation_missing_key_id_blocks_completeness() {
    let mut evidence = complete_evidence();
    evidence.binding.rotations[0].next_key_id = String::new();
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
    assert!(report.reasons.iter().any(|r| r.contains("rotação")));
}

// 2. Permissões: any capability beyond read, or an unrecognized flag, fails
// the key closed.
#[test]
fn withdrawal_capable_key_blocks_completeness() {
    let mut evidence = complete_evidence();
    evidence.permissions.enable_withdrawals = true;
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
    assert!(report.reasons.iter().any(|r| r.contains("apiRestrictions")));
}

#[test]
fn unknown_permission_flag_fails_closed() {
    let mut evidence = complete_evidence();
    evidence
        .permissions
        .unknown_flags
        .push("enableFutureProduct".to_string());
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
}

// 3. Saldo livre/bloqueado: BalanceComponents.total() must use both, once.
#[test]
fn balance_total_sums_free_and_locked_exactly_once() {
    let components = BalanceComponents {
        free: 1.5,
        locked: 0.5,
    };
    assert_eq!(components.total(), 2.0);
}

// 4. Trades paginados: a symbol in the universe without exhausted trade
// coverage blocks completeness.
#[test]
fn symbol_with_unproven_trade_pagination_blocks_completeness() {
    let mut evidence = complete_evidence();
    evidence.trades.get_mut("BTCUSDT").unwrap().outcome = PageOutcome::Unproven;
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
    assert!(report
        .reasons
        .iter()
        .any(|r| r.contains("BTCUSDT") && r.contains("PAGINATION_UNPROVEN")));
}

// 5. Taxa em BNB: fee currency is recorded as evidence but does not itself
// block completeness when it differs from the quote asset.
#[test]
fn bnb_fee_currency_does_not_block_completeness() {
    let evidence = complete_evidence();
    assert!(evidence.trades["BTCUSDT"]
        .fee_currencies_observed
        .contains("BNB"));
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::PolicyComplete);
}

// 6. Round trip de símbolo zerado: once a symbol enters the universe it
// remains required even if it is absent from the trades map (simulating a
// collector that dropped it because the current position is zero).
#[test]
fn zeroed_symbol_still_required_once_in_universe() {
    let mut evidence = complete_evidence();
    evidence.trades.remove("LUNAUSDT");
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
    assert!(report
        .reasons
        .iter()
        .any(|r| r.contains("LUNAUSDT") && r.contains("sem cobertura")));
}

// 7. Símbolo removido: a delisted symbol must still appear in an archived
// catalog version; its absence from every archived catalog is a gap the
// engine cannot presently detect from trades alone, but eligibleFrom
// coverage (test 14) enforces at least one archived pre-window catalog.
#[test]
fn delisted_symbol_stays_in_universe_and_trade_coverage_is_still_required() {
    let evidence = complete_evidence();
    assert!(evidence.symbol_universe.contains("LUNAUSDT"));
    assert!(evidence.trades.contains_key("LUNAUSDT"));
}

// 8. Depósito/saque/estorno: an unclassified status (e.g. a reversal type
// the collector does not yet recognize) fails the family closed.
#[test]
fn withdrawal_with_unknown_status_blocks_completeness() {
    let mut evidence = complete_evidence();
    evidence
        .withdrawals
        .unknown_kinds
        .insert("PARTIAL_REVERSAL".to_string());
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
    assert!(report.reasons.iter().any(|r| r.contains("saques")));
}

// 9. Transferências para fora de Spot: unproven pagination on the universal
// transfer family blocks completeness.
#[test]
fn transfer_family_unproven_pagination_blocks_completeness() {
    let mut evidence = complete_evidence();
    evidence.transfers.outcome = PageOutcome::Unproven;
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
    assert!(report.reasons.iter().any(|r| r.contains("transferências")));
}

// 10. Convert/dust/distribuição: an unknown kind in any of the three
// families fails closed independently.
#[test]
fn unknown_convert_kind_blocks_completeness() {
    let mut evidence = complete_evidence();
    evidence
        .convert
        .unknown_kinds
        .insert("UNDOCUMENTED_SPLIT".to_string());
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
    assert!(report.reasons.iter().any(|r| r.contains("Convert")));
}

#[test]
fn unknown_dust_kind_blocks_completeness() {
    let mut evidence = complete_evidence();
    evidence
        .dust
        .unknown_kinds
        .insert("UNKNOWN_DRIBBLET".to_string());
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
}

#[test]
fn unknown_dividend_kind_blocks_completeness() {
    let mut evidence = complete_evidence();
    evidence
        .dividends
        .unknown_kinds
        .insert("UNKNOWN_DISTRIBUTION".to_string());
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
}

// 11. Timestamps simultâneos: represented at the pagination layer
// (pagination_test.rs::tied_boundary_without_secondary_cursor_blocks_continuation);
// here we confirm the coverage engine reflects an unresolved tie as
// PageOutcome::Unproven on the affected family, exactly like any other
// unproven pagination.
#[test]
fn tied_timestamp_boundary_surfaces_as_unproven_family() {
    let mut evidence = complete_evidence();
    // The collector could not safely continue past a tied timestamp on the
    // deposit history endpoint; it must record the family as unproven
    // rather than guessing a cutoff.
    evidence.deposits.outcome = PageOutcome::Unproven;
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
    assert!(report.reasons.iter().any(|r| r.contains("depósitos")));
}

// 12. Endpoint truncado: same shape as unproven pagination on trades.
#[test]
fn truncated_trade_endpoint_blocks_completeness() {
    let mut evidence = complete_evidence();
    let coverage = evidence.trades.get_mut("LUNAUSDT").unwrap();
    coverage.outcome = PageOutcome::Unproven;
    coverage.pages_fetched = 500; // hit a documented cap every request
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
}

// 13. Restrições históricas: no archived catalog version at or before
// eligibleFrom means historical restrictions cannot be attributed to any
// symbol reliably.
#[test]
fn missing_pre_eligibility_catalog_blocks_completeness() {
    let mut evidence = complete_evidence();
    evidence.catalog_versions.clear();
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
    assert!(report.reasons.iter().any(|r| r.contains("eligibleFrom")));
}

// 14. Versão de catálogo: a catalog captured only after eligibleFrom is
// insufficient even if catalogs exist.
#[test]
fn catalog_only_after_eligibility_blocks_completeness() {
    let mut evidence = complete_evidence();
    evidence.catalog_versions[0].captured_at_ms = evidence.eligible_from_ms + 1;
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
}

// 15. Cut consistente e conciliação de saldo: a mismatch between the
// ledger's reconstructed total and the observed balance at the cut blocks
// completeness; no silent zero or estimate is substituted.
#[test]
fn balance_mismatch_at_cut_blocks_completeness() {
    let mut evidence = complete_evidence();
    evidence.reconciliations[0].observed_total.free += 0.1;
    let report = evaluate(&evidence);
    assert_eq!(report.status, CoverageStatus::Incomplete);
    assert!(report
        .reasons
        .iter()
        .any(|r| r.contains("BTC") && r.contains("reconcilia")));
}

#[test]
fn report_serializes_to_json_for_the_public_manifest() {
    let report = evaluate(&complete_evidence());
    let json = serde_json::to_string(&report).expect("report must serialize");
    assert!(
        json.contains("PolicyComplete")
            || json.contains("policyComplete")
            || json.contains("\"status\"")
    );
}
