use corrections::{apply_correction, VersionStatus};

#[test]
fn correction_preserves_the_original_digest_unchanged() {
    let original_digest = [1u8; 32];
    let replacement_digest = [2u8; 32];
    let applied = apply_correction(original_digest, replacement_digest);

    assert_eq!(
        applied.original.digest, original_digest,
        "the original's digest is never rewritten"
    );
    assert_eq!(applied.replacement.digest, replacement_digest);
}

#[test]
fn correction_marks_original_superseded_instead_of_deleting_it() {
    let applied = apply_correction([1u8; 32], [2u8; 32]);
    assert_eq!(
        applied.original.status,
        VersionStatus::Superseded { by: [2u8; 32] }
    );
    assert_eq!(applied.replacement.status, VersionStatus::Original);
}

#[test]
fn original_remains_retrievable_alongside_its_replacement() {
    // The applied correction carries both versions in the same value —
    // there is no operation that returns only the replacement and
    // discards the original.
    let applied = apply_correction([9u8; 32], [8u8; 32]);
    assert_ne!(applied.original.digest, applied.replacement.digest);
}
