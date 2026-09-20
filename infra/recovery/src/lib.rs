//! Infrastructure and recovery: a restore ensaio that measures
//! real RPO/RTO against targets, isolated per-category retention
//! deadlines, mirror-object verification by content hash, and the
//! 91-day purge gate that only ever allows purging raw inputs behind a
//! validated recursive receipt — otherwise retaining them integrally.

pub mod mirror;
pub mod purge_gate;
pub mod restore_drill;
pub mod retention;

pub use mirror::{verify_object_mirrored, MirrorMismatch};
pub use purge_gate::{evaluate_purge_gate, PurgeDecision, RecursiveReceiptStatus, SIMULATION_DAYS};
pub use restore_drill::{verify_restore_drill, RestoreDrill, RestoreFailure, RestoreTargets};
pub use retention::{
    check_purged_by_deadline, purge_deadline_ms, DataCategory, RetentionViolation,
};
