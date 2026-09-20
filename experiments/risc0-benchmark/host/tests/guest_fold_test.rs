//! Fast correctness check for the guest's fold, independent of proving.
//!
//! Runs the guest through the executor only (no proof generation, seconds
//! not minutes) against a small, hand-computed vector, so a change to the
//! fold logic is caught without paying for a full proving run. The
//! expensive proving path itself is exercised by `host/src/main.rs` /
//! `report.md`, which additionally confirms the real receipt verifies and
//! a fake one does not.

use methods::{METHODS_ELF, METHODS_ID};
use risc0_zkvm::ExecutorEnv;

// Mirrors methods/guest/src/main.rs::Event; kept as plain data here rather
// than shared via a dependency, since the guest crate targets riscv32im
// and is not meant to be linked natively by the host.
#[derive(serde::Serialize)]
struct Event {
    id: u64,
    delta: i64,
}

#[test]
fn guest_fold_matches_hand_computed_vector() {
    // balance = 100 - 30 + 5 = 75; fingerprint computed independently in
    // Python against the same wrapping FNV-1a-style fold as the guest.
    let events = vec![
        Event { id: 0, delta: 100 },
        Event { id: 1, delta: -30 },
        Event { id: 2, delta: 5 },
    ];
    let expected_balance: i64 = 75;
    let expected_fingerprint: u64 = 18_351_081_064_515_976_058;

    let env = ExecutorEnv::builder()
        .write(&events)
        .unwrap()
        .build()
        .unwrap();
    let executor = risc0_zkvm::default_executor();
    let session = executor.execute(env, METHODS_ELF).unwrap();

    let journal = session.journal;
    let (count, balance, fingerprint): (u32, i64, u64) = journal.decode().unwrap();

    assert_eq!(count, 3);
    assert_eq!(balance, expected_balance);
    assert_eq!(fingerprint, expected_fingerprint);
}

#[test]
fn guest_rejects_overflowing_balance() {
    // Two deltas that overflow i64 when summed must halt the guest rather
    // than silently wrap the reported balance.
    let events = vec![
        Event {
            id: 0,
            delta: i64::MAX,
        },
        Event { id: 1, delta: 1 },
    ];

    let env = ExecutorEnv::builder()
        .write(&events)
        .unwrap()
        .build()
        .unwrap();
    let executor = risc0_zkvm::default_executor();
    let result = executor.execute(env, METHODS_ELF);

    assert!(
        result.is_err(),
        "guest must fail on balance overflow instead of wrapping"
    );
}

#[test]
fn empty_batch_commits_zeroed_journal() {
    let events: Vec<Event> = vec![];

    let env = ExecutorEnv::builder()
        .write(&events)
        .unwrap()
        .build()
        .unwrap();
    let executor = risc0_zkvm::default_executor();
    let session = executor.execute(env, METHODS_ELF).unwrap();

    let journal = session.journal;
    let (count, balance, fingerprint): (u32, i64, u64) = journal.decode().unwrap();

    assert_eq!((count, balance, fingerprint), (0, 0, 0));
}

#[test]
fn methods_id_is_stable_non_zero() {
    // Sanity check that the compiled guest has a real image ID, guarding
    // against a build misconfiguration silently producing an empty ELF.
    assert_ne!(METHODS_ID, [0u32; 8]);
}
