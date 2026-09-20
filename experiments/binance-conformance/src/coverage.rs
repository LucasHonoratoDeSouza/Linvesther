//! Decides `POLICY_COMPLETE` / `INCOMPLETE` for a B0 homologation window
//! from collected [`Evidence`], and renders the public `CoverageManifest`.
//!
//! This module intentionally does not reconcile ledgers or normalize flows
//! (that is `crates/domain/ledger` in a later task); it only decides
//! whether the evidence collected is sufficient to claim complete coverage.

use crate::model::{Evidence, PageOutcome};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoverageStatus {
    PolicyComplete,
    Incomplete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    pub status: CoverageStatus,
    /// Every reason the status is not `PolicyComplete`; empty when complete.
    /// Ordered by evaluation so the first blocking condition is always
    /// first, but every condition is still checked and reported.
    pub reasons: Vec<String>,
}

const RECONCILIATION_EPSILON: f64 = 1e-9;

pub fn evaluate(evidence: &Evidence) -> CoverageReport {
    let mut reasons = Vec::new();

    if !evidence.permissions.is_read_only() {
        reasons.push(
            "apiRestrictions concede capacidade além de leitura, ou possui flag desconhecida"
                .to_string(),
        );
    }

    if evidence
        .binding
        .rotations
        .iter()
        .any(|rotation| rotation.previous_key_id.is_empty() || rotation.next_key_id.is_empty())
    {
        reasons.push("evento de rotação de chave sem key id anterior/novo".to_string());
    }

    for symbol in &evidence.symbol_universe {
        match evidence.trades.get(symbol) {
            None => reasons.push(format!(
                "símbolo '{symbol}' no universo sem cobertura de myTrades"
            )),
            Some(coverage) if coverage.outcome != PageOutcome::Exhausted => reasons.push(format!(
                "símbolo '{symbol}' com paginação de myTrades não provada (PAGINATION_UNPROVEN)"
            )),
            Some(_) => {}
        }
    }

    for (name, family) in [
        ("depósitos", &evidence.deposits),
        ("saques", &evidence.withdrawals),
        ("transferências universais", &evidence.transfers),
        ("Convert", &evidence.convert),
        ("dust conversions", &evidence.dust),
        ("dividendos/distribuições", &evidence.dividends),
    ] {
        if family.outcome != PageOutcome::Exhausted {
            reasons.push(format!("{name}: paginação não provada"));
        }
        if !family.unknown_kinds.is_empty() {
            reasons.push(format!(
                "{name}: tipo/status desconhecido não classificado ({:?})",
                family.unknown_kinds
            ));
        }
    }

    if !evidence
        .catalog_versions
        .iter()
        .any(|version| version.captured_at_ms <= evidence.eligible_from_ms)
    {
        reasons.push("nenhuma versão de exchangeInfo arquivada antes de eligibleFrom".to_string());
    }

    for reconciliation in &evidence.reconciliations {
        if !reconciliation
            .ledger_total
            .reconciles_with(&reconciliation.observed_total, RECONCILIATION_EPSILON)
        {
            reasons.push(format!(
                "ativo '{}': saldo observado não reconcilia com ledger no corte",
                reconciliation.asset
            ));
        }
    }

    let status = if reasons.is_empty() {
        CoverageStatus::PolicyComplete
    } else {
        CoverageStatus::Incomplete
    };
    CoverageReport { status, reasons }
}
