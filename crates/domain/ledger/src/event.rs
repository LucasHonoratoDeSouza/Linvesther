//! `LedgerEvent`, the canonical economic event this crate accumulates
//! into per-asset balances — per the protocol specification: "conta,
//! namespace de origem, ID de origem, timestamp econômico e de
//! observação, tipo, legs por ativo, fees, referência ao raw."
//!
//! Deliberately adapter-agnostic: `source_namespace` is an opaque string
//! the adapter defines (e.g. `"binance.trade"`, `"binance.deposit"`), not
//! an enum this crate hardcodes — the core does not import a venue SDK or
//! branch on vendor names (an architecture rule).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerLeg {
    pub asset: String,
    /// Decimal string (never `f64`); parsed by the ledger at ingest time.
    pub amount: String,
    pub is_credit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEvent {
    pub source_namespace: String,
    pub source_id: String,
    pub economic_time_ms: u64,
    pub observed_time_ms: u64,
    pub legs: Vec<LedgerLeg>,
    /// A fee, if this event carries one. Applied to the balance exactly
    /// once, alongside `legs` — never folded into them, never skipped.
    pub fee: Option<LedgerLeg>,
}

impl LedgerEvent {
    /// The full dedup/conflict-detection key: namespace + source id, per
    /// the protocol — the same key with different content is a conflict, not
    /// an overwrite.
    pub fn key(&self) -> (String, String) {
        (self.source_namespace.clone(), self.source_id.clone())
    }
}
