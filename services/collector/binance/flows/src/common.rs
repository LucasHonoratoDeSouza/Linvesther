//! Common output shape every flow family normalizes into.
//!
//! Mirrors the shape of `LedgerEvent` in the protocol specification
//! ("conta, namespace de origem, ID de origem, timestamp econômico...,
//! tipo, legs por ativo, fees, referência ao raw") at the granularity this
//! crate owns — account binding and raw-evidence references are a later
//! task's concern (the ledger itself); this crate's job stops at
//! producing correctly classified, reconciled legs from one flow record.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowFamily {
    Deposit,
    Withdrawal,
    Transfer,
    Convert,
    Dust,
    Dividend,
}

impl fmt::Display for FlowFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            FlowFamily::Deposit => "deposit",
            FlowFamily::Withdrawal => "withdrawal",
            FlowFamily::Transfer => "transfer",
            FlowFamily::Convert => "convert",
            FlowFamily::Dust => "dust",
            FlowFamily::Dividend => "dividend",
        };
        f.write_str(name)
    }
}

/// One asset movement. `amount` is an unsigned decimal string (never
/// `f64`); direction is `is_credit`, never encoded as a sign in the
/// string, so a leg can never be misread by dropping a `-`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetLeg {
    pub asset: String,
    pub amount: String,
    pub is_credit: bool,
}

impl AssetLeg {
    pub fn credit(asset: impl Into<String>, amount: impl Into<String>) -> Self {
        Self {
            asset: asset.into(),
            amount: amount.into(),
            is_credit: true,
        }
    }

    pub fn debit(asset: impl Into<String>, amount: impl Into<String>) -> Self {
        Self {
            asset: asset.into(),
            amount: amount.into(),
            is_credit: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedFlow {
    pub source_namespace: FlowFamily,
    pub source_id: String,
    pub economic_time_ms: u64,
    /// The family-specific status/type/reason this record was classified
    /// as (e.g. "Credited", "Completed", a transfer type code) —
    /// human-readable evidence of the classification, not itself used
    /// for further branching by callers.
    pub kind: String,
    /// Principal legs. A record whose principal is not yet recognized
    /// (e.g. a pending deposit) has no legs at all — never a leg with a
    /// silently-zeroed or estimated amount.
    pub legs: Vec<AssetLeg>,
    /// The fee leg, if this record type charges one. Always a debit when
    /// present (a fee is never credited to the account).
    pub fee: Option<AssetLeg>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FlowError {
    #[error("{family}: unrecognized status/type/reason code, fail-closed: {code}")]
    UnknownKind { family: FlowFamily, code: String },
}
