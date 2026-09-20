//! `GET /sapi/v1/asset/dribblet` normalization.
//!
//! Per the Binance adapter specification: "Transformações de ativos e taxas."
//! One dust conversion event debits one or more small-balance assets and
//! credits a single destination asset (Binance always converts dust into
//! BNB), with a service charge fee taken from the destination asset.

use crate::common::{AssetLeg, FlowError, FlowFamily, NormalizedFlow};

#[derive(Debug, Clone)]
pub struct RawDustDetail {
    pub from_asset: String,
    pub amount: String,
}

#[derive(Debug, Clone)]
pub struct RawDust {
    pub dribblet_id: String,
    pub to_asset: String,
    pub total_transferred: String,
    pub total_service_charge: String,
    pub details: Vec<RawDustDetail>,
    pub operate_time_ms: u64,
}

pub fn normalize_dust(raw: &RawDust) -> Result<NormalizedFlow, FlowError> {
    if raw.details.is_empty() {
        return Err(FlowError::UnknownKind {
            family: FlowFamily::Dust,
            code: "empty details".to_string(),
        });
    }

    let mut legs: Vec<AssetLeg> = raw
        .details
        .iter()
        .map(|d| AssetLeg::debit(d.from_asset.clone(), d.amount.clone()))
        .collect();
    legs.push(AssetLeg::credit(
        raw.to_asset.clone(),
        raw.total_transferred.clone(),
    ));

    let fee = if raw.total_service_charge != "0" && !raw.total_service_charge.is_empty() {
        Some(AssetLeg::debit(
            raw.to_asset.clone(),
            raw.total_service_charge.clone(),
        ))
    } else {
        None
    };

    Ok(NormalizedFlow {
        source_namespace: FlowFamily::Dust,
        source_id: raw.dribblet_id.clone(),
        economic_time_ms: raw.operate_time_ms,
        kind: "Completed".to_string(),
        legs,
        fee,
    })
}
