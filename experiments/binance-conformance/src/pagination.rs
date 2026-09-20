//! Pagination-exhaustion rules shared by every paginated endpoint family.
//!
//! Binance endpoints differ in cursor style (offset, `fromId`, `startTime`
//! window), but the exhaustion question is the same: can the collector prove
//! no record was skipped between this page and the next request?

use crate::model::PageOutcome;

/// Whether it is safe to issue the next paginated request at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryDecision {
    /// Either the page was short (nothing more to fetch) or the boundary
    /// between this page and the next is unambiguous.
    SafeToContinue,
    /// The page was full and its last records tie on timestamp with records
    /// that did not fit, with no secondary key to disambiguate. Advancing
    /// the window by one millisecond would silently drop the tied records
    /// that stayed on this side of the cut.
    BlockedByTie,
}

/// Decides whether the collector may safely request the next page.
///
/// `tied_with_next` is true when the last record's timestamp equals the
/// timestamp of records known to still be pending. `has_secondary_cursor`
/// is true when the endpoint also supports a strictly-increasing tiebreaker
/// (e.g. trade id) that lets the collector resume inside a tied timestamp
/// without loss or duplication.
pub fn resolve_boundary(
    returned: usize,
    page_size: usize,
    tied_with_next: bool,
    has_secondary_cursor: bool,
) -> BoundaryDecision {
    if returned < page_size {
        return BoundaryDecision::SafeToContinue;
    }
    if tied_with_next && !has_secondary_cursor {
        BoundaryDecision::BlockedByTie
    } else {
        BoundaryDecision::SafeToContinue
    }
}

/// Classifies a single page's exhaustion: a short page proves the query is
/// done; a full page never proves it by itself, since a full page can
/// always be followed by more records.
pub fn classify_page(returned: usize, page_size: usize) -> PageOutcome {
    if returned < page_size {
        PageOutcome::Exhausted
    } else {
        PageOutcome::Unproven
    }
}

/// Folds a full paginated query into a single verdict from its ordered
/// per-page outcomes and the boundary decision made after each page.
///
/// A query is exhausted only when it terminates in a short (or empty) page
/// — full intermediate pages are expected and do not themselves block
/// completeness — and no boundary along the way was blocked by an
/// undisambiguated timestamp tie. An empty query (no pages at all) is
/// unproven: the collector must observe at least one terminating response.
pub fn fold_query(
    page_outcomes: impl IntoIterator<Item = PageOutcome>,
    boundary_decisions: impl IntoIterator<Item = BoundaryDecision>,
) -> PageOutcome {
    let boundaries_safe = boundary_decisions
        .into_iter()
        .all(|decision| decision == BoundaryDecision::SafeToContinue);
    match (page_outcomes.into_iter().last(), boundaries_safe) {
        (Some(PageOutcome::Exhausted), true) => PageOutcome::Exhausted,
        _ => PageOutcome::Unproven,
    }
}
