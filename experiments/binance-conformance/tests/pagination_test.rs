use binance_conformance::model::PageOutcome;
use binance_conformance::pagination::{
    classify_page, fold_query, resolve_boundary, BoundaryDecision,
};

#[test]
fn short_page_is_exhausted() {
    assert_eq!(classify_page(37, 1000), PageOutcome::Exhausted);
}

#[test]
fn full_page_alone_is_unproven() {
    // Full page: the query cannot claim completeness without a follow-up
    // request, since Binance page limits are always achievable in practice.
    assert_eq!(classify_page(1000, 1000), PageOutcome::Unproven);
}

#[test]
fn empty_page_is_exhausted() {
    assert_eq!(classify_page(0, 1000), PageOutcome::Exhausted);
}

#[test]
fn tied_boundary_without_secondary_cursor_blocks_continuation() {
    let decision = resolve_boundary(1000, 1000, true, false);
    assert_eq!(decision, BoundaryDecision::BlockedByTie);
}

#[test]
fn tied_boundary_with_secondary_cursor_is_safe() {
    let decision = resolve_boundary(1000, 1000, true, true);
    assert_eq!(decision, BoundaryDecision::SafeToContinue);
}

#[test]
fn full_page_without_tie_is_safe_to_continue() {
    let decision = resolve_boundary(1000, 1000, false, false);
    assert_eq!(decision, BoundaryDecision::SafeToContinue);
}

#[test]
fn multi_page_query_exhausted_only_when_last_page_short_and_no_blocked_boundary() {
    let outcomes = [
        PageOutcome::Unproven,
        PageOutcome::Unproven,
        PageOutcome::Exhausted,
    ];
    let boundaries = [
        BoundaryDecision::SafeToContinue,
        BoundaryDecision::SafeToContinue,
    ];
    assert_eq!(fold_query(outcomes, boundaries), PageOutcome::Exhausted);
}

#[test]
fn multi_page_query_unproven_when_any_boundary_blocked_even_if_final_page_short() {
    let outcomes = [PageOutcome::Unproven, PageOutcome::Exhausted];
    let boundaries = [BoundaryDecision::BlockedByTie];
    assert_eq!(fold_query(outcomes, boundaries), PageOutcome::Unproven);
}

#[test]
fn query_with_no_pages_observed_is_unproven() {
    let outcomes: [PageOutcome; 0] = [];
    let boundaries: [BoundaryDecision; 0] = [];
    assert_eq!(fold_query(outcomes, boundaries), PageOutcome::Unproven);
}

#[test]
fn truncated_endpoint_never_seen_short_page_stays_unproven() {
    // Endpoint truncated at its documented maximum on every request: the
    // collector never observes a page proving the tail is empty.
    let outcomes = [PageOutcome::Unproven; 5];
    let boundaries = [BoundaryDecision::SafeToContinue; 4];
    assert_eq!(fold_query(outcomes, boundaries), PageOutcome::Unproven);
}
