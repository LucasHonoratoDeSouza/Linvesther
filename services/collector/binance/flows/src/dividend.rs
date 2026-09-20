//! `GET /sapi/v1/asset/assetDividend` normalization.
//!
//! Per the Binance adapter specification: "Classificar por motivo homologado;
//! tipo desconhecido bloqueia valuation." Like transfer types, which
//! dividend reason strings (e.g. `"Staking"`, `"Airdrop"`) this
//! deployment has actually homologated is an operational decision the
//! caller supplies via [`DividendReasonPolicy`], not a hardcoded list.

use crate::common::{AssetLeg, FlowError, FlowFamily, NormalizedFlow};
use std::collections::HashSet;

pub struct DividendReasonPolicy {
    homologated: HashSet<String>,
}

impl DividendReasonPolicy {
    pub fn new() -> Self {
        Self {
            homologated: HashSet::new(),
        }
    }

    pub fn allow(mut self, reason: impl Into<String>) -> Self {
        self.homologated.insert(reason.into());
        self
    }
}

impl Default for DividendReasonPolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct RawDividend {
    pub id: String,
    pub asset: String,
    pub amount: String,
    pub reason: String,
    pub div_time_ms: u64,
}

pub fn normalize_dividend(
    raw: &RawDividend,
    policy: &DividendReasonPolicy,
) -> Result<NormalizedFlow, FlowError> {
    if !policy.homologated.contains(&raw.reason) {
        return Err(FlowError::UnknownKind {
            family: FlowFamily::Dividend,
            code: raw.reason.clone(),
        });
    }

    Ok(NormalizedFlow {
        source_namespace: FlowFamily::Dividend,
        source_id: raw.id.clone(),
        economic_time_ms: raw.div_time_ms,
        kind: raw.reason.clone(),
        legs: vec![AssetLeg::credit(raw.asset.clone(), raw.amount.clone())],
        fee: None,
    })
}
