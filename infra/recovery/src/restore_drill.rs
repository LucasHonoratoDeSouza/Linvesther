//! Restore ensaio, per the operations design:
//! "Restore operacional | RPO ≤ 15 min, RTO ≤ 4 h, demonstrados por
//! ensaio."

#[derive(Debug, Clone, Copy)]
pub struct RestoreTargets {
    pub rpo_ms: i64,
    pub rto_ms: i64,
}

impl RestoreTargets {
    pub const DEFAULT: RestoreTargets = RestoreTargets {
        rpo_ms: 15 * 60_000,
        rto_ms: 4 * 3_600_000,
    };
}

/// One drill's real timestamps: when the last backup was taken, when the
/// failure happened, and when restore actually completed.
#[derive(Debug, Clone, Copy)]
pub struct RestoreDrill {
    pub last_backup_at_ms: i64,
    pub failure_at_ms: i64,
    pub restore_completed_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreFailure {
    RpoExceeded { actual_ms: i64, target_ms: i64 },
    RtoExceeded { actual_ms: i64, target_ms: i64 },
}

/// Checks a drill's *actual* measured timings against the targets — this
/// never assumes the targets were met; it derives RPO (time between the
/// last backup and the failure — the data that could have been lost) and
/// RTO (time between the failure and restore completing) from the
/// drill's own timestamps.
pub fn verify_restore_drill(
    drill: &RestoreDrill,
    targets: &RestoreTargets,
) -> Result<(), Vec<RestoreFailure>> {
    let mut failures = Vec::new();

    let data_loss_window_ms = drill.failure_at_ms - drill.last_backup_at_ms;
    if data_loss_window_ms > targets.rpo_ms {
        failures.push(RestoreFailure::RpoExceeded {
            actual_ms: data_loss_window_ms,
            target_ms: targets.rpo_ms,
        });
    }

    let downtime_ms = drill.restore_completed_at_ms - drill.failure_at_ms;
    if downtime_ms > targets.rto_ms {
        failures.push(RestoreFailure::RtoExceeded {
            actual_ms: downtime_ms,
            target_ms: targets.rto_ms,
        });
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}
