// Guest workload for the RISC Zero feasibility benchmark.
//
// This is not the reconciliation guest of the reference protocol; it is a
// bounded, representative fold over a batch of synthetic ledger-shaped
// events, sized to measure guest execution and proving cost at the 10k
// scale required by the spec, without committing to reconciliation
// semantics that belong to a later task.

use risc0_zkvm::guest::env;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    pub delta: i64,
}

fn main() {
    let events: Vec<Event> = env::read();

    let mut balance: i64 = 0;
    let mut fingerprint: u64 = 0;
    let mut count: u32 = 0;

    for event in &events {
        balance = balance
            .checked_add(event.delta)
            .expect("balance overflow in benchmark fold");
        fingerprint = fingerprint
            .wrapping_mul(1_099_511_628_211) // FNV-1a prime
            ^ event.id.wrapping_add(event.delta as u64);
        count += 1;
    }

    env::commit(&(count, balance, fingerprint));
}
