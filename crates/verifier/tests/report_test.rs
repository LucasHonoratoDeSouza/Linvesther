use verifier::report::{CheckOutcome, Mode, VerificationReport};
use verifier::OriginAssessment;

fn all_pass_report(mode: Mode) -> VerificationReport {
    VerificationReport {
        mode,
        hashes: CheckOutcome::Pass,
        account_set_root: CheckOutcome::Pass,
        calculation: CheckOutcome::Pass,
        registry: CheckOutcome::Pass,
        coverage: CheckOutcome::Pass,
        origin: OriginAssessment::SelfAttested { fingerprint: [0u8; 32] },
    }
}

#[test]
fn offline_success_is_reported_as_valid_as_of_never_as_current() {
    let report = all_pass_report(Mode::Offline {
        as_of_ms: 1_700_000_000_000,
    });
    let summary = report.summary();
    assert_eq!(summary, "VALID_AS_OF(1700000000000)");
    assert!(!summary.contains("CURRENT"));
    assert_ne!(summary, "VALID");
}

#[test]
fn online_success_is_reported_as_valid() {
    let report = all_pass_report(Mode::Online);
    assert_eq!(report.summary(), "VALID");
}

#[test]
fn any_single_failing_check_makes_the_whole_report_invalid_regardless_of_mode() {
    let mut report = all_pass_report(Mode::Offline { as_of_ms: 0 });
    report.calculation = CheckOutcome::Fail("tampered image ID".to_string());
    assert!(!report.all_pass());
    assert_eq!(report.summary(), "INVALID");
}

#[test]
fn an_unavailable_check_never_counts_as_a_pass() {
    let mut report = all_pass_report(Mode::Offline { as_of_ms: 0 });
    report.registry = CheckOutcome::Unavailable("no anchor in bundle".to_string());
    assert!(!report.all_pass());
}

#[test]
fn a_passing_calculation_never_reads_as_a_claim_about_the_data() {
    let mut report = all_pass_report(Mode::Online);
    assert_eq!(report.summary(), "VALID");
    assert_eq!(report.origin_summary(), "ORIGIN=SELF_ATTESTED");
    assert!(!report.origin.is_trusted());

    report.origin = OriginAssessment::TrustedCollector { name: "example".to_string(), fingerprint: [1u8; 32] };
    assert_eq!(report.origin_summary(), "ORIGIN=TRUSTED_COLLECTOR(example)");
    assert!(report.origin.is_trusted());
}
