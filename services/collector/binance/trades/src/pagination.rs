//! Pagination-exhaustion primitives for `myTrades`, per
//! the Binance adapter specification: "percorrer myTrades até fronteira
//! demonstrada... Critério é exaustão da consulta autenticada, não `id+1`
//! observado em toda linha... Página cheia exige continuação/subdivisão;
//! limite truncado sem evidência de término é `PAGINATION_UNPROVEN`."
//!
//! Conceptually the same exhaustion rules as
//! `experiments/binance-conformance/src/pagination.rs` (a short/empty page
//! proves exhaustion, a full page never does by itself), reimplemented
//! here as the module a real collector actually calls per page fetched —
//! that crate is a standalone conformance experiment, this one is the
//! collector's own pagination state.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageOutcome {
    Exhausted,
    Unproven,
}

/// Classifies one page: short/empty proves exhaustion; full does not.
pub fn classify_page(returned: usize, page_capacity: usize) -> PageOutcome {
    if returned < page_capacity {
        PageOutcome::Exhausted
    } else {
        PageOutcome::Unproven
    }
}

/// Tracks pagination state for one symbol's `myTrades` query across
/// however many pages are fetched.
#[derive(Debug, Clone)]
pub struct PaginationTracker {
    pages_fetched: u32,
    outcome: PageOutcome,
}

impl Default for PaginationTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PaginationTracker {
    pub fn new() -> Self {
        // No page fetched yet: unproven by definition — the collector
        // must observe at least one terminating (short/empty) page.
        Self {
            pages_fetched: 0,
            outcome: PageOutcome::Unproven,
        }
    }

    /// Records one fetched page. The tracker's outcome after this call
    /// reflects only the *latest* page: a short page after any number of
    /// full pages proves exhaustion (full intermediate pages are
    /// expected, not a problem by themselves); a full page — even after
    /// a previously short one — reopens the question. This mirrors real
    /// pagination: state is about "is there more to fetch right now."
    pub fn record_page(&mut self, returned: usize, page_capacity: usize) {
        self.pages_fetched += 1;
        self.outcome = classify_page(returned, page_capacity);
    }

    pub fn pages_fetched(&self) -> u32 {
        self.pages_fetched
    }

    pub fn outcome(&self) -> PageOutcome {
        self.outcome
    }

    pub fn is_exhausted(&self) -> bool {
        self.outcome == PageOutcome::Exhausted
    }
}
