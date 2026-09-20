//! A0 origin attestation: the signer covers commitments, policy,
//! binding and the interval of a `SourceEnvelope` in a single digest, so
//! changing any one field invalidates the signature, and the origin
//! badge derived from the envelope's declared mechanism always stays A0
//! for an A0 envelope regardless of what else is true about it.

pub mod badge;
pub mod envelope;
pub mod signer;

pub use badge::{origin_badge, OriginBadge};
pub use envelope::SourceEnvelope;
pub use signer::{collector_fingerprint, verify, A0Signer, SignerError};
