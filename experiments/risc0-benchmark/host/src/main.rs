//! RISC Zero feasibility benchmark.
//!
//! Proves the guest's fold over a synthetic batch of ledger-shaped events at
//! the 10k-event scale required by the spec, measures wall-clock proving
//! time (p50/p95 across repeated runs), guest cycle counts, receipt size and
//! peak resident memory, and demonstrates that a receipt which did not go
//! through real proving is rejected by verification.
//!
//! Configuration via environment variables (all optional):
//! - `BENCH_EVENTS` (default `10000`): events per proving run.
//! - `BENCH_REPS` (default `3`): full end-to-end proving repetitions used
//!   for the wall-clock percentile estimate. Proving is expensive, so this
//!   sample is small; see `report.md` for the caveat that implies.

use methods::{METHODS_ELF, METHODS_ID};
use risc0_zkvm::{
    default_prover, ExecutorEnv, FakeReceipt, InnerReceipt, MaybePruned, Receipt, ReceiptClaim,
    VerifierContext,
};
use std::env as std_env;
use std::fs;
use std::time::{Duration, Instant};

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct Event {
    id: u64,
    delta: i64,
}

/// Deterministic xorshift64 generator so the benchmark is reproducible
/// without embedding real financial data or requiring a `rand` dependency.
struct Xorshift64(u64);

impl Xorshift64 {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fn synthetic_events(count: usize, seed: u64) -> Vec<Event> {
    let mut rng = Xorshift64(seed | 1);
    (0..count)
        .map(|i| Event {
            id: i as u64,
            // Bounded, signed deltas: representative magnitude for a
            // per-event balance movement without claiming real semantics.
            delta: (rng.next() % 2_000_000) as i64 - 1_000_000,
        })
        .collect()
}

fn peak_rss_kb() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")
            .map(|rest| rest.trim().trim_end_matches(" kB").trim().parse().ok())
            .flatten()
    })
}

fn percentile(sorted: &[Duration], fraction: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let index = ((sorted.len() as f64 - 1.0) * fraction).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn run_execution_only(events: &Vec<Event>) -> u64 {
    let env = ExecutorEnv::builder()
        .write(events)
        .unwrap()
        .build()
        .unwrap();
    let executor = risc0_zkvm::default_executor();
    let session = executor.execute(env, METHODS_ELF).unwrap();
    session
        .segments
        .iter()
        .map(|segment| segment.cycles as u64)
        .sum()
}

fn run_prove(events: &Vec<Event>) -> (Duration, risc0_zkvm::ProveInfo) {
    let env = ExecutorEnv::builder()
        .write(events)
        .unwrap()
        .build()
        .unwrap();
    let prover = default_prover();
    let start = Instant::now();
    let prove_info = prover.prove(env, METHODS_ELF).unwrap();
    (start.elapsed(), prove_info)
}

/// Builds a receipt that did not go through real proving — reusing the
/// genuine journal bytes from an already-proven run, so only the proof
/// itself is fake — and confirms the verifier rejects it. This is the "fake
/// receipt rejeitada" gate item: a `FakeReceipt` only ever passes
/// verification when the verifier itself is explicitly put in dev mode,
/// which production verification must never do.
fn assert_fake_receipt_is_rejected(journal_bytes: Vec<u8>) {
    let claim = ReceiptClaim::ok(METHODS_ID, journal_bytes.clone());
    let fake = FakeReceipt::new(MaybePruned::Value(claim));
    let receipt = Receipt::new(InnerReceipt::Fake(fake), journal_bytes);

    // Explicitly non-dev-mode context: this is the context production
    // verification must use, irrespective of any ambient RISC0_DEV_MODE
    // environment variable in the process that produced the receipt.
    let ctx = VerifierContext::default();
    let result = receipt.verify_with_context(&ctx, METHODS_ID);
    assert!(
        result.is_err(),
        "a fake receipt must never pass verification outside dev mode"
    );
    println!(
        "fake receipt correctly rejected: {}",
        result.unwrap_err()
    );
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let event_count: usize = std_env::var("BENCH_EVENTS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10_000);
    let reps: usize = std_env::var("BENCH_REPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3)
        .max(1);

    let events = synthetic_events(event_count, 0x5EED_5EED_5EED_5EEDu64);

    let cycles = run_execution_only(&events);

    let mut durations = Vec::with_capacity(reps);
    let mut last_prove_info: Option<risc0_zkvm::ProveInfo> = None;
    for rep in 0..reps {
        let (elapsed, prove_info) = run_prove(&events);
        eprintln!("rep {}/{}: {:?}", rep + 1, reps, elapsed);
        durations.push(elapsed);
        last_prove_info = Some(prove_info);
    }
    durations.sort();

    let prove_info = last_prove_info.expect("at least one repetition runs");
    let receipt = &prove_info.receipt;

    receipt
        .verify(METHODS_ID)
        .expect("the real receipt produced by proving must verify");

    let (count, _balance, _fingerprint): (u32, i64, u64) = receipt.journal.decode().unwrap();
    assert_eq!(count as usize, event_count);

    assert_fake_receipt_is_rejected(receipt.journal.bytes.clone());

    let receipt_bytes = bincode::serialize(receipt).expect("receipt must serialize");

    let p50 = percentile(&durations, 0.50);
    let p95 = percentile(&durations, 0.95);
    let rss_kb = peak_rss_kb();

    println!("--- risc0-benchmark report ---");
    println!("events: {event_count}");
    println!("repetitions: {reps} (small sample; p95 approximated by the observed max)");
    println!("execution cycles (unproven run): {cycles}");
    println!("proving stats (last rep): {:?}", prove_info.stats);
    println!("receipt size: {} bytes", receipt_bytes.len());
    println!("proving wall time p50: {p50:?}");
    println!("proving wall time p95: {p95:?}");
    println!("proving wall time min/max: {:?} / {:?}", durations.first(), durations.last());
    match rss_kb {
        Some(kb) => println!("peak RSS (VmHWM): {kb} kB"),
        None => println!("peak RSS: unavailable (non-Linux /proc)"),
    }
    println!("fake receipt: rejected as expected");
}
