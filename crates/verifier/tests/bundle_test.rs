use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use verifier::{Bundle, Manifest, ManifestEntry};

fn hash_hex(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

fn sample_bundle() -> Bundle {
    let mut files = BTreeMap::new();
    files.insert(
        "identity.json".to_string(),
        r#"{"identityId":"id-1"}"#.to_string(),
    );
    files.insert(
        "accounts.json".to_string(),
        r#"{"members":["acc-1"]}"#.to_string(),
    );

    let mut manifest_files = BTreeMap::new();
    for (path, content) in &files {
        manifest_files.insert(
            path.clone(),
            ManifestEntry {
                hash: hash_hex(content),
                size_bytes: content.len() as u64,
            },
        );
    }
    Bundle {
        manifest: Manifest {
            version: "1".to_string(),
            files: manifest_files,
        },
        files,
    }
}

#[test]
fn an_untampered_bundle_verifies_cleanly() {
    let bundle = sample_bundle();
    assert!(bundle.verify_hashes().is_empty());
}

#[test]
fn a_tampered_file_is_detected() {
    let mut bundle = sample_bundle();
    bundle.files.insert(
        "identity.json".to_string(),
        r#"{"identityId":"id-EVIL"}"#.to_string(),
    );

    let mismatches = bundle.verify_hashes();
    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].path, "identity.json");
}

#[test]
fn a_file_missing_despite_being_manifested_is_detected() {
    let mut bundle = sample_bundle();
    bundle.files.remove("accounts.json");

    let mismatches = bundle.verify_hashes();
    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].path, "accounts.json");
    assert_eq!(mismatches[0].actual, "<missing file>");
}
