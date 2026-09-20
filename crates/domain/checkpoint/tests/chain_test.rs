use checkpoint::{link_next, ChainError, Checkpoint, CheckpointState};

fn checkpoint(sequence: u64, previous_digest: Option<[u8; 32]>, digest: [u8; 32]) -> Checkpoint {
    Checkpoint {
        sequence,
        previous_digest,
        digest,
        interval_start_ms: 0,
        interval_end_ms: 1,
        state: CheckpointState::Committed,
    }
}

#[test]
fn next_checkpoint_links_to_the_previous_one() {
    let genesis = checkpoint(0, None, [1; 32]);
    let next = checkpoint(1, Some([1; 32]), [2; 32]);
    assert!(link_next(&genesis, &next).is_ok());
}

#[test]
fn wrong_sequence_is_rejected() {
    let genesis = checkpoint(0, None, [1; 32]);
    let next = checkpoint(2, Some([1; 32]), [2; 32]);
    assert_eq!(
        link_next(&genesis, &next),
        Err(ChainError::SequenceMismatch {
            expected: 1,
            got: 2
        })
    );
}

#[test]
fn wrong_previous_digest_is_rejected() {
    let genesis = checkpoint(0, None, [1; 32]);
    let next = checkpoint(1, Some([9; 32]), [2; 32]);
    assert_eq!(
        link_next(&genesis, &next),
        Err(ChainError::PreviousDigestMismatch)
    );
}

#[test]
fn missing_previous_digest_is_rejected_for_a_non_genesis_checkpoint() {
    let genesis = checkpoint(0, None, [1; 32]);
    let next = checkpoint(1, None, [2; 32]);
    assert_eq!(
        link_next(&genesis, &next),
        Err(ChainError::PreviousDigestMismatch)
    );
}

#[test]
fn a_three_checkpoint_chain_links_end_to_end() {
    let c0 = checkpoint(0, None, [1; 32]);
    let c1 = checkpoint(1, Some([1; 32]), [2; 32]);
    let c2 = checkpoint(2, Some([2; 32]), [3; 32]);
    assert!(link_next(&c0, &c1).is_ok());
    assert!(link_next(&c1, &c2).is_ok());
}
