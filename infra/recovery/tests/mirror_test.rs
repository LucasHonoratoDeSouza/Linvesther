use recovery::verify_object_mirrored;

#[test]
fn an_identical_mirror_verifies() {
    let primary = b"receipt-bytes-here";
    let mirror = b"receipt-bytes-here".to_vec();
    assert!(verify_object_mirrored(primary, &mirror).is_ok());
}

#[test]
fn a_diverged_mirror_is_detected() {
    let primary = b"receipt-bytes-here";
    let mirror = b"receipt-bytes-DIFFERENT";
    let result = verify_object_mirrored(primary, mirror);
    assert!(result.is_err());
    let mismatch = result.unwrap_err();
    assert_ne!(mismatch.primary_hash, mismatch.mirror_hash);
}

#[test]
fn a_truncated_mirror_is_detected() {
    let primary = b"receipt-bytes-here";
    let mirror = b"receipt-bytes-her"; // one byte short
    assert!(verify_object_mirrored(primary, mirror).is_err());
}
