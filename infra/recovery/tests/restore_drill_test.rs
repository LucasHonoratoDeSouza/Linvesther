use recovery::{verify_restore_drill, RestoreDrill, RestoreFailure, RestoreTargets};

const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 3_600_000;

#[test]
fn a_drill_within_both_targets_passes() {
    let drill = RestoreDrill {
        last_backup_at_ms: 0,
        failure_at_ms: 10 * MINUTE_MS,
        restore_completed_at_ms: 10 * MINUTE_MS + 2 * HOUR_MS,
    };
    assert!(verify_restore_drill(&drill, &RestoreTargets::DEFAULT).is_ok());
}

#[test]
fn a_data_loss_window_over_the_rpo_target_fails() {
    let drill = RestoreDrill {
        last_backup_at_ms: 0,
        failure_at_ms: 20 * MINUTE_MS,
        restore_completed_at_ms: 20 * MINUTE_MS + HOUR_MS,
    };
    let result = verify_restore_drill(&drill, &RestoreTargets::DEFAULT);
    assert_eq!(
        result,
        Err(vec![RestoreFailure::RpoExceeded {
            actual_ms: 20 * MINUTE_MS,
            target_ms: 15 * MINUTE_MS
        }])
    );
}

#[test]
fn downtime_over_the_rto_target_fails() {
    let drill = RestoreDrill {
        last_backup_at_ms: 0,
        failure_at_ms: 5 * MINUTE_MS,
        restore_completed_at_ms: 5 * MINUTE_MS + 5 * HOUR_MS,
    };
    let result = verify_restore_drill(&drill, &RestoreTargets::DEFAULT);
    assert_eq!(
        result,
        Err(vec![RestoreFailure::RtoExceeded {
            actual_ms: 5 * HOUR_MS,
            target_ms: 4 * HOUR_MS
        }])
    );
}

#[test]
fn both_targets_missed_are_both_reported() {
    let drill = RestoreDrill {
        last_backup_at_ms: 0,
        failure_at_ms: 20 * MINUTE_MS,
        restore_completed_at_ms: 20 * MINUTE_MS + 5 * HOUR_MS,
    };
    let result = verify_restore_drill(&drill, &RestoreTargets::DEFAULT);
    assert_eq!(result.unwrap_err().len(), 2);
}

#[test]
fn exactly_at_the_target_boundary_passes() {
    let drill = RestoreDrill {
        last_backup_at_ms: 0,
        failure_at_ms: 15 * MINUTE_MS,
        restore_completed_at_ms: 15 * MINUTE_MS + 4 * HOUR_MS,
    };
    assert!(verify_restore_drill(&drill, &RestoreTargets::DEFAULT).is_ok());
}
