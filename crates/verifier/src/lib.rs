//! Public verifier: checks a portable bundle entirely from
//! local data — no official API call required. Five independent checks
//! (tamper, root, image, chain, coverage) each produce their own
//! pass/fail reason rather than a single opaque boolean, and offline
//! verification is always reported as `VALID_AS_OF(snapshot)`, never as
//! a claim of current validity.

pub mod anchor;
pub mod bundle;
pub mod coverage;
pub mod receipt;
pub mod report;
pub mod root;
pub mod trust;
pub mod verify;

pub use anchor::{verify_anchor, AnchorError, TrustedAnchor};
pub use bundle::{Bundle, HashMismatch, Manifest, ManifestEntry};
pub use coverage::{verify_coverage, CoverageMismatch};
pub use receipt::{decode_journal, decode_journal_bytes, guest_image_id, verify_receipt, PerformanceJournal, ReceiptError};
pub use report::{CheckOutcome, Mode, VerificationReport};
pub use root::verify_account_set_root;
pub use trust::{assess_origin, OriginAssessment, TrustList, TrustListError, TrustedCollector};
pub use verify::{verify, VerificationInput};
