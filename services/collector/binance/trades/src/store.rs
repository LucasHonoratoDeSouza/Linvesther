//! Trade storage and per-symbol pagination tracking.
//!
//! Per the protocol specification: "QUANDO um evento se repete,
//! DEVE resultar no mesmo registro; mesmo ID com conteúdo diferente DEVE
//! gerar conflito, sem sobrescrever original." A retry that re-fetches an
//! already-ingested trade is idempotent only if the content matches
//! exactly; any difference is a conflict, and the original is kept.

use crate::pagination::{PageOutcome, PaginationTracker};
use crate::trade::Trade;
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum IngestError {
    #[error("trade {symbol}#{id} already recorded with different content")]
    Conflict { symbol: String, id: u64 },
}

pub struct TradeStore {
    // Keyed first by symbol, so the same numeric id under a different
    // symbol lives in a completely separate map — a structural guarantee
    // against cross-symbol collision, not just a convention.
    by_symbol: BTreeMap<String, BTreeMap<u64, Trade>>,
    pagination: BTreeMap<String, PaginationTracker>,
}

impl Default for TradeStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TradeStore {
    pub fn new() -> Self {
        Self {
            by_symbol: BTreeMap::new(),
            pagination: BTreeMap::new(),
        }
    }

    /// Ingests one page of trades for `symbol`. `page_capacity` is the
    /// endpoint's documented page size; `trades.len()` is compared
    /// against it to classify this page's pagination outcome. Returns a
    /// conflict error (without ingesting anything from this page) at the
    /// first trade whose id was already recorded with different content
    /// — a partial, silently-truncated ingest would be worse than
    /// refusing the whole page.
    pub fn ingest_page(
        &mut self,
        symbol: &str,
        trades: Vec<Trade>,
        page_capacity: usize,
    ) -> Result<(), IngestError> {
        let symbol_trades = self.by_symbol.entry(symbol.to_string()).or_default();
        for trade in &trades {
            if let Some(existing) = symbol_trades.get(&trade.id) {
                if existing != trade {
                    return Err(IngestError::Conflict {
                        symbol: symbol.to_string(),
                        id: trade.id,
                    });
                }
            }
        }
        for trade in trades.iter().cloned() {
            symbol_trades.entry(trade.id).or_insert(trade);
        }

        self.pagination
            .entry(symbol.to_string())
            .or_default()
            .record_page(trades.len(), page_capacity);
        Ok(())
    }

    pub fn trades_for(&self, symbol: &str) -> Vec<&Trade> {
        self.by_symbol
            .get(symbol)
            .map(|m| m.values().collect())
            .unwrap_or_default()
    }

    pub fn trade(&self, symbol: &str, id: u64) -> Option<&Trade> {
        self.by_symbol.get(symbol).and_then(|m| m.get(&id))
    }

    pub fn symbols(&self) -> impl Iterator<Item = &String> {
        self.by_symbol.keys()
    }

    /// `None` if `symbol` has never had a page ingested at all — distinct
    /// from having ingested pages but still being unproven.
    pub fn pagination_outcome(&self, symbol: &str) -> Option<PageOutcome> {
        self.pagination.get(symbol).map(|t| t.outcome())
    }

    pub fn is_symbol_exhausted(&self, symbol: &str) -> bool {
        self.pagination
            .get(symbol)
            .is_some_and(|t| t.is_exhausted())
    }

    pub fn pages_fetched(&self, symbol: &str) -> u32 {
        self.pagination
            .get(symbol)
            .map(|t| t.pages_fetched())
            .unwrap_or(0)
    }
}
