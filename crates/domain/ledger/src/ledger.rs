//! The ledger accumulator: ingests [`LedgerEvent`]s, reconciles per-asset
//! balances, rejects conflicting re-ingests, and counts a fee exactly
//! once.

use crate::event::{LedgerEvent, LedgerLeg};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::str::FromStr;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LedgerError {
    #[error("event {namespace}#{source_id} already recorded with different content")]
    Conflict {
        namespace: String,
        source_id: String,
    },
    #[error("leg for asset {asset} has an unparseable decimal amount: {amount}")]
    InvalidAmount { asset: String, amount: String },
}

pub struct Ledger {
    events: BTreeMap<(String, String), LedgerEvent>,
    balances: BTreeMap<String, Decimal>,
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

impl Ledger {
    pub fn new() -> Self {
        Self {
            events: BTreeMap::new(),
            balances: BTreeMap::new(),
        }
    }

    /// Ingests one event. A retry re-ingesting the identical event
    /// (same namespace/id, same content) is a no-op. An event with the
    /// same namespace/id but different content is rejected — the
    /// original stays authoritative, the balance is not touched, and the
    /// error names exactly which record conflicted.
    ///
    /// All leg amounts are parsed and validated *before* any balance is
    /// mutated: a malformed amount anywhere in the event rejects the
    /// whole event, never a partial application.
    pub fn ingest(&mut self, event: LedgerEvent) -> Result<(), LedgerError> {
        let key = event.key();
        if let Some(existing) = self.events.get(&key) {
            if existing == &event {
                return Ok(());
            }
            return Err(LedgerError::Conflict {
                namespace: key.0,
                source_id: key.1,
            });
        }

        let mut deltas: Vec<(String, Decimal)> = Vec::with_capacity(event.legs.len() + 1);
        for leg in event.legs.iter().chain(event.fee.iter()) {
            deltas.push((leg.asset.clone(), signed_amount(leg)?));
        }

        for (asset, delta) in deltas {
            *self.balances.entry(asset).or_insert(Decimal::ZERO) += delta;
        }
        self.events.insert(key, event);
        Ok(())
    }

    pub fn balance(&self, asset: &str) -> Decimal {
        self.balances.get(asset).copied().unwrap_or(Decimal::ZERO)
    }

    pub fn event(&self, namespace: &str, source_id: &str) -> Option<&LedgerEvent> {
        self.events
            .get(&(namespace.to_string(), source_id.to_string()))
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    pub fn assets(&self) -> impl Iterator<Item = &String> {
        self.balances.keys()
    }
}

fn signed_amount(leg: &LedgerLeg) -> Result<Decimal, LedgerError> {
    let amount = Decimal::from_str(&leg.amount).map_err(|_| LedgerError::InvalidAmount {
        asset: leg.asset.clone(),
        amount: leg.amount.clone(),
    })?;
    Ok(if leg.is_credit { amount } else { -amount })
}
