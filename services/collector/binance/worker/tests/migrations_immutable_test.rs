//! An applied migration must never change.
//!
//! `sqlx` records a SHA-384 of every migration it applies and refuses to start
//! if the file later differs, even by a comment. The worker then fails on every
//! call, and every connected account looks missing. A change to the schema goes
//! in a new migration; this test fails, and says so, when an existing file is
//! edited.

use sha2::{Digest, Sha384};
use std::fs;
use std::path::Path;

/// The checksum of each migration as it was applied. Add a line for each new
/// migration; never edit a line.
const APPLIED: &[(&str, &str)] = &[
    ("0001_init.sql", "7291343f576c8734e36fb780f859ad6f0125927fa837c2e9b852519ec352e69dae1a8e65a0f03e4f375eefe56381b6f6"),
    ("0002_connection_label.sql", "13ba5fbdcc6c60cbc48c12fc44b786d959ddc66c75905a08bdef9088462b690a46ec6bbe3486949d2307156cebb2f706"),
    ("0003_coinbase.sql", "99a9c8bd3587205bd5c07e1f37976dc18603671ade56f9d846c132efe23288e241be72d5cbd33e5e3d47baf6d144a593"),
    ("0004_ibkr.sql", "ea455adffa36ce50b290d0a3517155017810eab0130f0c9e980bb455e16ac80617fe906e4bac38f2ce1b3ae72a6802dd"),
    ("0005_kraken.sql", "1a234ca1d44e11acba14780dbf53c11dae25ba941a82d06d4baf7ce63d6386b7fa19e95d19daf3200ee5e026f2d1602a"),
];

#[test]
fn applied_migrations_have_not_been_edited() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for (name, expected) in APPLIED {
        let bytes = fs::read(dir.join(name)).unwrap_or_else(|e| panic!("cannot read migration {name}: {e}"));
        let actual = hex::encode(Sha384::digest(&bytes));
        assert_eq!(
            &actual, expected,
            "{name} was edited after it was applied. sqlx will refuse to start against a database that has \
             the old version, and the worker will fail on every call. Restore the file and put the change in a new migration."
        );
    }
}

#[test]
fn every_migration_on_disk_is_recorded() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let recorded: Vec<&str> = APPLIED.iter().map(|(name, _)| *name).collect();
    for entry in fs::read_dir(&dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        if name.ends_with(".sql") {
            assert!(
                recorded.contains(&name.as_str()),
                "{name} is not recorded in APPLIED: add its checksum so a later edit is caught"
            );
        }
    }
}
