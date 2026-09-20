//! Bundle loading and hash verification, mirroring
//! `apps/api/src/exports/publicBundle.ts` — this crate cannot
//! import that TypeScript code (different language, different
//! workspace), so it reimplements the same manifest/hash scheme
//! independently in Rust. Both compute `sha256(file content)` over the
//! same UTF-8 bytes, so a bundle produced by the API export verifies here as-is.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub hash: String,
    #[serde(rename = "sizeBytes")]
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub files: BTreeMap<String, ManifestEntry>,
}

#[derive(Debug, Clone)]
pub struct Bundle {
    pub manifest: Manifest,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashMismatch {
    pub path: String,
    pub expected: String,
    pub actual: String,
}

fn sha256_hex(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

impl Bundle {
    /// Recomputes every manifested file's hash from its actual content —
    /// the "tamper" check. A file whose content was altered after the
    /// manifest was written, or a file the manifest lists but the
    /// bundle doesn't carry, is reported, never silently accepted.
    pub fn verify_hashes(&self) -> Vec<HashMismatch> {
        let mut mismatches = Vec::new();
        for (path, entry) in &self.manifest.files {
            match self.files.get(path) {
                None => mismatches.push(HashMismatch {
                    path: path.clone(),
                    expected: entry.hash.clone(),
                    actual: "<missing file>".to_string(),
                }),
                Some(content) => {
                    let actual = sha256_hex(content);
                    if actual != entry.hash {
                        mismatches.push(HashMismatch {
                            path: path.clone(),
                            expected: entry.hash.clone(),
                            actual,
                        });
                    }
                }
            }
        }
        mismatches
    }

    pub fn file(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }
}
