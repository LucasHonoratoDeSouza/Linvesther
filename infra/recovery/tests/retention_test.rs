use recovery::{check_purged_by_deadline, purge_deadline_ms, DataCategory};

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;

#[test]
fn credential_deadline_is_24_hours() {
    assert_eq!(purge_deadline_ms(DataCategory::Credential, 0), 24 * HOUR_MS);
}

#[test]
fn private_active_deadline_is_30_days() {
    assert_eq!(
        purge_deadline_ms(DataCategory::PrivateActive, 0),
        30 * DAY_MS
    );
}

#[test]
fn backup_deadline_is_35_days() {
    assert_eq!(purge_deadline_ms(DataCategory::Backup, 0), 35 * DAY_MS);
}

#[test]
fn data_purged_before_its_deadline_is_fine() {
    assert!(check_purged_by_deadline(DataCategory::Credential, 0, false, 23 * HOUR_MS).is_ok());
}

#[test]
fn data_still_present_past_its_deadline_is_a_violation() {
    let result = check_purged_by_deadline(DataCategory::Credential, 0, true, 25 * HOUR_MS);
    assert!(result.is_err());
}

#[test]
fn a_credential_overdue_does_not_imply_a_backup_is_overdue() {
    // Same "now": credential is well past its 24h deadline, but backups
    // (35 days) have not even started their own clock in a meaningful
    // way at this timescale.
    let now_ms = 25 * HOUR_MS;
    assert!(check_purged_by_deadline(DataCategory::Credential, 0, true, now_ms).is_err());
    assert!(check_purged_by_deadline(DataCategory::Backup, 0, true, now_ms).is_ok());
}
