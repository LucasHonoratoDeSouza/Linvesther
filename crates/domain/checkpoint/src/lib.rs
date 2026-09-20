//! Checkpoint composition and gaps: calendar-derived gap detection
//! that needs no voluntary user event, late recovery that never restores
//! continuity retroactively, and a next-checkpoint link that must chain
//! to the previous one.

pub mod calendar;
pub mod chain;
pub mod continuity;

pub use calendar::{derive_gaps, CalendarPolicy, Gap};
pub use chain::{link_next, ChainError, Checkpoint, CheckpointState};
pub use continuity::{recover_late, segments_between, LateRecoveryError, LateSupplement, Segment};
