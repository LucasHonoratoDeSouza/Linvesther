//! Local execution boundary,: "QUANDO coleta/prova é
//! local, credenciais e witness NÃO DEVEM sair para o operador; mesmos
//! dados DEVERÃO produzir mesmos resultados canônicos."

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Everything that must never leave this machine: the collector
/// credential and the witness data a real proof would need. Kept in one
/// type so it is obvious, at a glance, what `execute_locally` must never
/// reference when building its output.
pub struct LocalOnlyInput {
    pub api_key: String,
    pub api_secret: String,
    pub witness_bytes: Vec<u8>,
    /// The data actually being processed — the only field
    /// `OperatorSafeResult` is derived from.
    pub ledger_events: Vec<LedgerEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEvent {
    pub id: String,
    pub delta: i64,
}

/// What is safe to send to the operator: derived only from
/// `ledger_events`, never from `api_key`/`api_secret`/`witness_bytes` —
/// those fields are simply not fields of this type, so nothing here can
/// reference them by construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorSafeResult {
    pub event_count: usize,
    pub balance: i64,
    pub fingerprint: u64,
}

/// Runs the computation locally and returns only the operator-safe
/// result. Naming the safe fields (rather than deleting the unsafe ones
/// from a copy) means a credential/witness field added to
/// `LocalOnlyInput` later cannot leak into this result silently.
pub fn execute_locally(input: &LocalOnlyInput) -> OperatorSafeResult {
    let mut balance: i64 = 0;
    let mut fingerprint: u64 = 0;
    for event in &input.ledger_events {
        balance += event.delta;
        fingerprint = fingerprint.wrapping_mul(1_099_511_628_211)
            ^ Sha256::digest(event.id.as_bytes())
                .iter()
                .fold(0u64, |acc, byte| acc.wrapping_add(u64::from(*byte)));
    }
    OperatorSafeResult {
        event_count: input.ledger_events.len(),
        balance,
        fingerprint,
    }
}

/// The exact bytes that would be sent to the operator — used both to
/// compute the canonical result digest and, in tests, to scan for any
/// leaked secret.
pub fn canonical_result_bytes(result: &OperatorSafeResult) -> Vec<u8> {
    serde_json::to_vec(result).expect("OperatorSafeResult always serializes")
}

pub fn canonical_result_digest(result: &OperatorSafeResult) -> [u8; 32] {
    Sha256::digest(canonical_result_bytes(result)).into()
}
