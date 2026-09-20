//! Checkpoint sequencing, per the protocol specification's definition of
//! `Checkpoint`: "sequência, anterior, intervalo, conjunto, raízes,
//! método, estado, prazo, prova e referência de ancoragem."

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointState {
    Collecting,
    Reconciled,
    Committed,
    Anchored,
    Finalized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checkpoint {
    pub sequence: u64,
    /// `None` only for the first checkpoint in a track.
    pub previous_digest: Option<[u8; 32]>,
    pub digest: [u8; 32],
    pub interval_start_ms: i64,
    pub interval_end_ms: i64,
    pub state: CheckpointState,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChainError {
    #[error("checkpoint sequence {got} does not follow {expected}")]
    SequenceMismatch { expected: u64, got: u64 },
    #[error("checkpoint's previous_digest does not match the actual previous checkpoint's digest")]
    PreviousDigestMismatch,
}

/// Confirms `next` links to `previous`: the sequence must increment by
/// one, and `next.previous_digest` must equal `Some(previous.digest)`. A
/// checkpoint that fails this can never be accepted as the track's next
/// state — it is not linked to anything, no matter what it otherwise
/// claims.
pub fn link_next(previous: &Checkpoint, next: &Checkpoint) -> Result<(), ChainError> {
    let expected_sequence = previous.sequence + 1;
    if next.sequence != expected_sequence {
        return Err(ChainError::SequenceMismatch {
            expected: expected_sequence,
            got: next.sequence,
        });
    }
    if next.previous_digest != Some(previous.digest) {
        return Err(ChainError::PreviousDigestMismatch);
    }
    Ok(())
}
