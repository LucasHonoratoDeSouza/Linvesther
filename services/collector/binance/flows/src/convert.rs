//! `GET /sapi/v1/convert/tradeFlow` normalization.
//!
//! Per the Binance adapter specification: "Tratar como troca interna + custos,
//! não depósito/saque fictícios" — always two legs (debit the source
//! asset, credit the destination asset), never framed as an external
//! deposit or withdrawal.

use crate::common::{AssetLeg, FlowError, FlowFamily, NormalizedFlow};

#[derive(Debug, Clone)]
pub struct RawConvert {
    pub quote_id: String,
    pub from_asset: String,
    pub from_amount: String,
    pub to_asset: String,
    pub to_amount: String,
    pub status: String,
    pub create_time_ms: u64,
}

fn classify_status(status: &str) -> Option<&'static str> {
    match status {
        "SUCCESS" => Some("Completed"),
        "PROCESS" => Some("Requested"),
        "FAIL" => Some("Failed"),
        _ => None,
    }
}

pub fn normalize_convert(raw: &RawConvert) -> Result<NormalizedFlow, FlowError> {
    let kind = classify_status(&raw.status).ok_or_else(|| FlowError::UnknownKind {
        family: FlowFamily::Convert,
        code: raw.status.clone(),
    })?;

    let legs = if kind == "Completed" {
        vec![
            AssetLeg::debit(raw.from_asset.clone(), raw.from_amount.clone()),
            AssetLeg::credit(raw.to_asset.clone(), raw.to_amount.clone()),
        ]
    } else {
        vec![]
    };

    Ok(NormalizedFlow {
        source_namespace: FlowFamily::Convert,
        source_id: raw.quote_id.clone(),
        economic_time_ms: raw.create_time_ms,
        kind: kind.to_string(),
        legs,
        fee: None,
    })
}
