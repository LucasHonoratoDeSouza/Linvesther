use corrections::{apply_reports, Report, Version, VersionStatus};

fn original(digest: [u8; 32]) -> Version {
    Version {
        digest,
        status: VersionStatus::Original,
    }
}

fn report(target: [u8; 32], allegation: &str) -> Report {
    Report {
        target_digest: target,
        allegation: allegation.to_string(),
    }
}

#[test]
fn a_single_unauthorized_report_does_not_invalidate() {
    let version = original([1u8; 32]);
    let result = apply_reports(version, &[report([1u8; 32], "seems off")]);
    assert_eq!(
        result, version,
        "a bare allegation carries no authority, so it changes nothing"
    );
}

#[test]
fn no_volume_of_reports_invalidates_without_admissible_evidence() {
    let version = original([1u8; 32]);
    let reports: Vec<Report> = (0..1000)
        .map(|i| report([1u8; 32], &format!("complaint {i}")))
        .collect();
    let result = apply_reports(version, &reports);
    assert_eq!(
        result, version,
        "1000 reports carry exactly as much authority as one: none"
    );
}

#[test]
fn reports_against_an_unrelated_digest_also_change_nothing() {
    let version = original([1u8; 32]);
    let result = apply_reports(version, &[report([2u8; 32], "wrong target entirely")]);
    assert_eq!(result, version);
}
