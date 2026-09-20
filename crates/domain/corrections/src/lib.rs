//! Corrections and derived invalidation: a correction preserves the
//! original version it targets, a report with no authority never
//! invalidates anything, the owner cannot veto validated evidence, and
//! conflicting corrections without a demonstrated supersession stay
//! `DISPUTED` rather than picking a favorable winner.

pub mod authority;
pub mod dispute;
pub mod report;
pub mod version;

pub use authority::{is_admissible, Authority, Correction, OwnerObjection};
pub use dispute::{reconcile_conflicting, Resolution, Supersession};
pub use report::{apply_reports, Report};
pub use version::{apply_correction, AppliedCorrection, Version, VersionStatus};
