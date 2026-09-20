//! `GET /sapi/v1/capital/withdraw/history` normalization.
//!
//! Per the Binance adapter specification: "Distinguir solicitação, débito,
//! conclusão, fee, falha e estorno."

use crate::common::{AssetLeg, FlowError, FlowFamily, NormalizedFlow};

#[derive(Debug, Clone)]
pub struct RawWithdrawal {
    pub id: String,
    pub asset: String,
    pub amount: String,
    pub transaction_fee: String,
    /// Binance's documented numeric status code.
    pub status: u32,
    pub apply_time_ms: u64,
}

fn classify_status(code: u32) -> Option<&'static str> {
    match code {
        0 => Some("Requested"), // email sent
        2 => Some("Requested"), // awaiting approval
        4 => Some("Requested"), // processing
        6 => Some("Completed"), // debited + completed
        1 => Some("Reversed"),  // cancelled: principal returned, never debited
        3 => Some("Failed"),    // rejected
        5 => Some("Failed"),    // failure
        _ => None,
    }
}

/// A withdrawal only debits the account once it actually completes; a
/// requested-but-not-yet-processed withdrawal has no legs yet (the funds
/// have not left), and a failed/reversed one never debits at all.
pub fn normalize_withdrawal(raw: &RawWithdrawal) -> Result<NormalizedFlow, FlowError> {
    let kind = classify_status(raw.status).ok_or_else(|| FlowError::UnknownKind {
        family: FlowFamily::Withdrawal,
        code: raw.status.to_string(),
    })?;

    let (legs, fee) = if kind == "Completed" {
        let legs = vec![AssetLeg::debit(raw.asset.clone(), raw.amount.clone())];
        let fee = if raw.transaction_fee != "0" && !raw.transaction_fee.is_empty() {
            Some(AssetLeg::debit(
                raw.asset.clone(),
                raw.transaction_fee.clone(),
            ))
        } else {
            None
        };
        (legs, fee)
    } else {
        (vec![], None)
    };

    Ok(NormalizedFlow {
        source_namespace: FlowFamily::Withdrawal,
        source_id: raw.id.clone(),
        economic_time_ms: raw.apply_time_ms,
        kind: kind.to_string(),
        legs,
        fee,
    })
}
