//! `GET /sapi/v1/asset/transfer` normalization.
//!
//! Per the Binance adapter specification: "Enumerar tipos permitidos na
//! política que cruzam Spot." Universal transfer type codes are numerous
//! and policy-defined (which ones the reference actually recognizes as
//! crossing the Spot perimeter is an operational decision, not a fact
//! this crate should hardcode from memory) — so the caller supplies a
//! [`TransferTypePolicy`] mapping recognized type codes to their
//! direction. A type code absent from the policy is unrecognized and
//! fails closed, exactly as an unrecognized status would.

use crate::common::{AssetLeg, FlowError, FlowFamily, NormalizedFlow};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    IntoSpot,
    OutOfSpot,
}

/// Maps Binance universal-transfer `type` codes (e.g.
/// `"MAIN_FUNDING"`, `"FUNDING_MAIN"`) this deployment recognizes as
/// crossing the Spot perimeter, to their direction. Not populated with
/// real Binance type codes here — see `README.md`.
pub struct TransferTypePolicy {
    directions: HashMap<String, TransferDirection>,
}

impl TransferTypePolicy {
    pub fn new() -> Self {
        Self {
            directions: HashMap::new(),
        }
    }

    pub fn allow(mut self, type_code: impl Into<String>, direction: TransferDirection) -> Self {
        self.directions.insert(type_code.into(), direction);
        self
    }
}

impl Default for TransferTypePolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct RawTransfer {
    pub tran_id: String,
    pub asset: String,
    pub amount: String,
    pub transfer_type: String,
    pub timestamp_ms: u64,
}

pub fn normalize_transfer(
    raw: &RawTransfer,
    policy: &TransferTypePolicy,
) -> Result<NormalizedFlow, FlowError> {
    let direction = policy
        .directions
        .get(&raw.transfer_type)
        .copied()
        .ok_or_else(|| FlowError::UnknownKind {
            family: FlowFamily::Transfer,
            code: raw.transfer_type.clone(),
        })?;

    let leg = match direction {
        TransferDirection::IntoSpot => AssetLeg::credit(raw.asset.clone(), raw.amount.clone()),
        TransferDirection::OutOfSpot => AssetLeg::debit(raw.asset.clone(), raw.amount.clone()),
    };

    Ok(NormalizedFlow {
        source_namespace: FlowFamily::Transfer,
        source_id: raw.tran_id.clone(),
        economic_time_ms: raw.timestamp_ms,
        kind: raw.transfer_type.clone(),
        legs: vec![leg],
        fee: None,
    })
}
