//! TLS origin composition guest: the badge this guest outputs is
//! computed from which verification path actually ran inside it, never
//! taken from an input field the caller could just assert. There is no
//! code path from `origin_verified_in_guest = false` (a bridge that
//! merely attests a translated digest, per proofs.md's "Composição sem
//! lacuna de confiança") to `OriginBadge::A1` — see `run` below.

pub mod session;
pub mod verify;

use serde::{Deserialize, Serialize};
use session::TlsSessionReceipt;
use sha2::{Digest, Sha256};
use verify::SessionError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestInput {
    pub receipt: TlsSessionReceipt,
    pub verifying_key_sec1: Vec<u8>,
    pub normalized_input_bytes: Vec<u8>,
    pub expected_hostname: String,
    /// Whether TLS origin verification is claimed to run inside this
    /// guest (`true`) versus having happened externally, with only a
    /// translated digest handed to a bridge (`false`). This is the only
    /// input that steers which badge is reachable, and it only ever
    /// narrows what's reachable — it can never itself grant `A1`.
    pub origin_verified_in_guest: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OriginBadge {
    A1,
    BridgeAttested,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestOutput {
    pub badge: OriginBadge,
    pub normalized_input_digest: [u8; 32],
}

#[derive(Debug)]
pub enum GuestError {
    Session(SessionError),
}

impl From<SessionError> for GuestError {
    fn from(value: SessionError) -> Self {
        GuestError::Session(value)
    }
}

/// A bridge path (`origin_verified_in_guest = false`) never runs the
/// session checks at all and always yields `OriginBadge::BridgeAttested`
/// — there is no branch, flag, or receipt content that flips this to
/// `A1`. Only the in-guest path, which genuinely re-derives and checks
/// the session (including that the normalized input matches exactly
/// what the session captured — the "raw vs. normalized swap" check),
/// can yield `A1`.
pub fn run(input: &GuestInput) -> Result<GuestOutput, GuestError> {
    if !input.origin_verified_in_guest {
        let normalized_input_digest = Sha256::digest(&input.normalized_input_bytes).into();
        return Ok(GuestOutput {
            badge: OriginBadge::BridgeAttested,
            normalized_input_digest,
        });
    }

    verify::verify_session_shape(&input.receipt, &input.expected_hostname)?;
    verify::verify_wallet_binding(&input.receipt, &input.verifying_key_sec1)?;
    verify::verify_raw_bound_to_input(&input.receipt, &input.normalized_input_bytes)?;

    let normalized_input_digest = Sha256::digest(&input.normalized_input_bytes).into();
    Ok(GuestOutput {
        badge: OriginBadge::A1,
        normalized_input_digest,
    })
}
