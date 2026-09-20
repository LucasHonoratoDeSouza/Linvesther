//! Cross-language conformance: reproduces every value in
//! `fixtures/commitments/vectors.json`, the same fixture the TypeScript
//! suite in `packages/protocol` checks against. A mismatch here means the
//! two languages would produce different commitments for identical inputs.

use commitments::*;
use serde_json::Value;
use std::fs;

fn vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/commitments/vectors.json"
    );
    let text = fs::read_to_string(path).expect("fixture file must be readable");
    serde_json::from_str(&text).expect("fixture file must be valid JSON")
}

fn hex32(s: &str) -> [u8; 32] {
    let bytes = hex::decode(s).expect("valid hex");
    bytes.try_into().expect("32 bytes")
}

#[test]
fn canonical_encoding_matches_fixture() {
    let v = vectors();
    let input = &v["canonical"]["input"];
    let expected = v["canonical"]["expectedBytesUtf8"].as_str().unwrap();
    let actual = canonicalize_value(input).unwrap();
    assert_eq!(String::from_utf8(actual).unwrap(), expected);
}

#[test]
fn frame_hash_matches_fixture() {
    let v = vectors();
    let tag = v["frameHash"]["tag"].as_str().unwrap();
    let fields: Vec<Vec<u8>> = v["frameHash"]["fieldsUtf8"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap().as_bytes().to_vec())
        .collect();
    let field_refs: Vec<&[u8]> = fields.iter().map(|f| f.as_slice()).collect();
    let expected = hex32(v["frameHash"]["expectedHex"].as_str().unwrap());
    assert_eq!(frame_hash(tag, &field_refs).unwrap(), expected);
}

#[test]
fn data_commitment_matches_fixture_and_reacts_to_salt_and_field_changes() {
    let v = vectors();
    let data = &v["dataCommitment"]["data"];
    let cases = v["dataCommitment"]["cases"].as_array().unwrap();

    let mut commitments = Vec::new();
    for case in cases {
        let salt = hex32(case["saltHex"].as_str().unwrap());
        let expected = hex32(case["expectedHex"].as_str().unwrap());
        let actual = data_commitment(&salt, data).unwrap();
        assert_eq!(actual, expected, "case {}", case["name"]);
        commitments.push(actual);
    }
    // Changing only the salt must change the commitment.
    assert_ne!(commitments[0], commitments[1]);

    let field_changed = &v["dataCommitment"]["fieldChangedCase"];
    let salt = hex32(field_changed["saltHex"].as_str().unwrap());
    let changed_data = &field_changed["data"];
    let expected = hex32(field_changed["expectedHex"].as_str().unwrap());
    let actual = data_commitment(&salt, changed_data).unwrap();
    assert_eq!(actual, expected);
    // Changing only a field (same salt as the baseline case) must change
    // the commitment too.
    assert_ne!(actual, commitments[0]);
}

#[test]
fn merkle_tree_matches_fixture() {
    let v = vectors();
    let m = &v["merkle"];
    let records = m["leafRecords"].as_array().unwrap();
    let salts_hex = m["leafSaltsHex"].as_array().unwrap();

    let leaves: Vec<[u8; 32]> = records
        .iter()
        .zip(salts_hex)
        .map(|(record, salt_hex)| {
            let salt = hex32(salt_hex.as_str().unwrap());
            leaf_hash(&salt, record).unwrap()
        })
        .collect();

    let expected_leaf_hashes: Vec<[u8; 32]> = m["expectedLeafHashesHex"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| hex32(h.as_str().unwrap()))
        .collect();
    assert_eq!(leaves, expected_leaf_hashes);

    let tree = MerkleTree::build(leaves.clone()).unwrap();
    assert_eq!(tree.root, hex32(m["expectedRootHex"].as_str().unwrap()));
    assert_eq!(tree.height() as u64, m["expectedHeight"].as_u64().unwrap());

    let proof_fixture = &m["inclusionProof"];
    let index = proof_fixture["index"].as_u64().unwrap();
    let proof = tree.prove(index).unwrap();
    let expected_siblings: Vec<[u8; 32]> = proof_fixture["expectedSiblingsHex"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| hex32(h.as_str().unwrap()))
        .collect();
    assert_eq!(proof.siblings, expected_siblings);
    assert_eq!(
        proof.count,
        m["leafRecords"].as_array().unwrap().len() as u64
    );

    verify_inclusion(&leaves[index as usize], &proof, &tree.root).expect("valid proof must verify");

    let empty_tree = MerkleTree::build(vec![]).unwrap();
    assert_eq!(
        empty_tree.root,
        hex32(m["emptyTree"]["expectedRootHex"].as_str().unwrap())
    );

    let single_tree = MerkleTree::build(vec![leaves[0]]).unwrap();
    assert_eq!(
        single_tree.root,
        hex32(m["singleLeafTree"]["expectedRootHex"].as_str().unwrap())
    );
    assert_eq!(
        single_tree.height() as u64,
        m["singleLeafTree"]["expectedHeight"].as_u64().unwrap()
    );
}
