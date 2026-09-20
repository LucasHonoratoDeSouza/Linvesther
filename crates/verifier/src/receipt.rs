//! Real receipt verification ("image" failure mode), per
//! the protocol specification's verification algorithm step 7:
//! "Validar receipt real, image ID, journal e equality de todos os
//! digests; rejeitar modo de desenvolvimento/fake receipt." Reuses the
//! real guest image ID from `zkvm-methods` — this is the same
//! `GUEST_ID` the real proving/verification test in that crate checks
//! against, not a value chosen by this crate.

use risc0_zkvm::Receipt;
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ReceiptError {
    #[error("could not decode the receipt bytes: {0}")]
    Decode(#[from] bincode::Error),
    #[error("receipt does not verify against image ID {image_id:?}: {reason}")]
    VerificationFailed { image_id: [u32; 8], reason: String },
    #[error("receipt's journal does not match the expected journal bytes")]
    JournalMismatch,
    #[error("the journal does not have the expected layout: {0}")]
    JournalLayout(String),
}

/// Decodes `receipt_bytes` (the same `bincode` encoding
/// `experiments/risc0-benchmark` and `zkvm/methods/performance` use),
/// verifies it against `image_id` with a real (non-dev-mode) verifier
/// context, and confirms its journal is byte-for-byte
/// `expected_journal_bytes` — a receipt that verifies for the *wrong*
/// image ID, or whose journal was swapped after proving, is rejected
/// here, not accepted because *some* valid-looking receipt was present.
pub fn verify_receipt(
    receipt_bytes: &[u8],
    image_id: [u32; 8],
    expected_journal_bytes: &[u8],
) -> Result<(), ReceiptError> {
    let receipt: Receipt = bincode::deserialize(receipt_bytes)?;
    receipt
        .verify(image_id)
        .map_err(|error| ReceiptError::VerificationFailed {
            image_id,
            reason: error.to_string(),
        })?;
    if receipt.journal.bytes != expected_journal_bytes {
        return Err(ReceiptError::JournalMismatch);
    }
    Ok(())
}

pub fn guest_image_id() -> [u32; 8] {
    zkvm_methods::GUEST_ID
}

/// What the performance guest commits to its journal. The field order and
/// types mirror the guest's `GuestOutput` exactly (the guest is its own
/// workspace, so the struct is duplicated); a golden journal in
/// `tests/receipt_test.rs`, also asserted by the guest's own tests, fails
/// if the two drift apart.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PerformanceJournal {
    pub envelope_digest: [u8; 32],
    pub signer_fingerprint: [u8; 32],
    pub period_start_ms: i64,
    pub period_end_ms: i64,
    pub twr_index_scaled: i128,
    pub mdd_bp: i64,
    pub capital: i64,
}

/// Decodes the journal of `receipt_bytes`. Call only on a receipt that has
/// already verified: the journal of an unverified receipt proves nothing.
pub fn decode_journal(receipt_bytes: &[u8]) -> Result<PerformanceJournal, ReceiptError> {
    let receipt: Receipt = bincode::deserialize(receipt_bytes)?;
    decode_journal_bytes(&receipt.journal.bytes)
}

pub fn decode_journal_bytes(journal_bytes: &[u8]) -> Result<PerformanceJournal, ReceiptError> {
    let journal = risc0_zkvm::Journal::new(journal_bytes.to_vec());
    journal.decode().map_err(|error| ReceiptError::JournalLayout(error.to_string()))
}
