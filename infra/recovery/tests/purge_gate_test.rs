use recovery::{evaluate_purge_gate, PurgeDecision, RecursiveReceiptStatus, SIMULATION_DAYS};

#[test]
fn before_the_simulation_window_raw_is_always_retained_regardless_of_receipt() {
    let decision = evaluate_purge_gate(SIMULATION_DAYS - 1, RecursiveReceiptStatus::Validated);
    assert!(matches!(decision, PurgeDecision::RetainIntegral { .. }));
}

#[test]
fn at_or_past_91_days_without_a_validated_receipt_still_retains_integrally() {
    let decision = evaluate_purge_gate(SIMULATION_DAYS, RecursiveReceiptStatus::NotAvailable);
    assert!(matches!(decision, PurgeDecision::RetainIntegral { .. }));

    let decision_later =
        evaluate_purge_gate(SIMULATION_DAYS + 100, RecursiveReceiptStatus::NotAvailable);
    assert!(matches!(
        decision_later,
        PurgeDecision::RetainIntegral { .. }
    ));
}

#[test]
fn at_or_past_91_days_with_a_validated_receipt_purge_is_permitted() {
    let decision = evaluate_purge_gate(SIMULATION_DAYS, RecursiveReceiptStatus::Validated);
    assert_eq!(decision, PurgeDecision::Purge);
}

#[test]
fn this_codebase_has_no_recursive_composition_implemented_yet_so_every_real_call_retains() {
    // Documents the current, honest state: nothing in this repo produces
    // a `RecursiveReceiptStatus::Validated` value yet (the performance guest only
    // implements the initial, non-recursive guest) — so today, this gate
    // always retains raw inputs integrally, exactly as
    // the operations design requires when recursion isn't implemented/validated.
    let decision = evaluate_purge_gate(365, RecursiveReceiptStatus::NotAvailable);
    assert!(matches!(decision, PurgeDecision::RetainIntegral { .. }));
}
