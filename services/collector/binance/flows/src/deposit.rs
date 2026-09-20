//! `GET /sapi/v1/capital/deposit/hisrec` normalization.
//!
//! Per the Binance adapter specification: "só reconhecer principal quando
//! creditado no perímetro." A pending or failed deposit produces a
//! record with **no legs** — never a leg with an estimated or zero
//! amount standing in for "not yet credited."

use crate::common::{AssetLeg, FlowError, FlowFamily, NormalizedFlow};

#[derive(Debug, Clone)]
pub struct RawDeposit {
    pub tx_id: String,
    pub asset: String,
    pub amount: String,
    /// Binance's documented numeric status code.
    pub status: u32,
    pub insert_time_ms: u64,
}

fn classify_status(code: u32) -> Option<&'static str> {
    match code {
        0 => Some("Pending"),
        6 => Some("Credited"), // "credited but cannot withdraw"
        1 => Some("Credited"), // success
        2 => Some("Pending"), // re-scan pending / ambiguous, treated conservatively as not yet credited
        7 => Some("WrongDeposit"),
        8 => Some("Pending"), // waiting user confirm
        _ => None,
    }
}

pub fn normalize_deposit(raw: &RawDeposit) -> Result<NormalizedFlow, FlowError> {
    let kind = classify_status(raw.status).ok_or_else(|| FlowError::UnknownKind {
        family: FlowFamily::Deposit,
        code: raw.status.to_string(),
    })?;

    let legs = if kind == "Credited" {
        vec![AssetLeg::credit(raw.asset.clone(), raw.amount.clone())]
    } else {
        vec![]
    };

    Ok(NormalizedFlow {
        source_namespace: FlowFamily::Deposit,
        source_id: raw.tx_id.clone(),
        economic_time_ms: raw.insert_time_ms,
        kind: kind.to_string(),
        legs,
        fee: None,
    })
}
