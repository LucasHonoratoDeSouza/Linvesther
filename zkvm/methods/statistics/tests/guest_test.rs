//! End-to-end tests for the statistics claims guest: real proving
//! that a claim approval survives round-trip through the zkVM, plus
//! fast execution-only tests covering sample/precision enforcement —
//! both via genuine RISC Zero execution, not a fake receipt.
//!
//! `MetricClaim`/`GuestInput`/`GuestOutput`/`ClaimOperator`/
//! `ClaimVerdict` are duplicated here from `guest/src/{lib,stats}.rs`
//! for the same cross-workspace reason documented in the performance and TLS composition guests' own
//! host-side test files.

use risc0_zkvm::{default_executor, default_prover, ExecutorEnv};
use serde::{Deserialize, Serialize};
use zkvm_methods_statistics::GUEST_ELF;

const SCALE: i128 = 1_000_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum ClaimOperator {
    GreaterThan(i128),
    LessThan(i128),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum ClaimVerdict {
    Approved,
    Rejected,
    IndeterminatePrecision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum MetricClaim {
    Sharpe {
        returns: Vec<i128>,
        rf_daily: i128,
        operator: ClaimOperator,
    },
    #[allow(dead_code)]
    Sortino {
        returns: Vec<i128>,
        mar_daily: i128,
        operator: ClaimOperator,
    },
    Cagr {
        index_start: i128,
        index_end: i128,
        days_elapsed: u32,
        operator: ClaimOperator,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuestInput {
    claim: MetricClaim,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GuestOutput {
    bracket_lower: i128,
    bracket_upper: i128,
    verdict: ClaimVerdict,
}

fn alternating_returns(n: usize) -> Vec<i128> {
    (0..n)
        .map(|i| {
            if i % 2 == 0 {
                SCALE / 100
            } else {
                -SCALE / 200
            }
        })
        .collect()
}

/// Real proving: a Sharpe claim that's clearly true (threshold well
/// below the certified bracket) is proven and its journal still says
/// `Approved` — the verdict isn't computed only by the fast executor
/// path, it's what a real zkVM proof actually commits.
#[test]
#[ignore = "real RISC Zero proving needs more free RAM than this machine reliably has alongside a normal desktop session; run explicitly with `cargo test -p zkvm-methods-statistics --test guest_test -- --ignored --test-threads=1` when the machine has headroom. `execution_*` tests below cover the same guest logic without proving."]
fn a_real_proof_of_an_approved_sharpe_claim_earns_the_verdict_for_real() {
    let input = GuestInput {
        claim: MetricClaim::Sharpe {
            returns: alternating_returns(60),
            rf_daily: 0,
            operator: ClaimOperator::GreaterThan(-SCALE),
        },
    };
    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();
    let prover = default_prover();
    let prove_info = prover
        .prove(env, GUEST_ELF)
        .expect("real proving of a well-formed sharpe claim must succeed");
    let receipt = &prove_info.receipt;
    receipt
        .verify(zkvm_methods_statistics::GUEST_ID)
        .expect("a real receipt for its own image ID must verify");

    let output: GuestOutput = receipt
        .journal
        .decode()
        .expect("journal decodes to GuestOutput");
    assert_eq!(output.verdict, ClaimVerdict::Approved);
}

/// Execution only: an insufficient CAGR sample panics the guest
/// (session failure) rather than publishing a fabricated bracket.
#[test]
fn execution_rejects_an_insufficient_cagr_sample_without_proving() {
    let input = GuestInput {
        claim: MetricClaim::Cagr {
            index_start: SCALE,
            index_end: SCALE * 2,
            days_elapsed: 100,
            operator: ClaimOperator::GreaterThan(0),
        },
    };
    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();
    let executor = default_executor();
    let result = executor.execute(env, GUEST_ELF);
    assert!(
        result.is_err(),
        "the guest must panic when the sample is fewer than 365 days"
    );
}

/// Execution only: a threshold that straddles the certified bracket
/// commits `IndeterminatePrecision`, not a guessed approval/rejection.
#[test]
fn execution_reports_indeterminate_precision_for_a_straddling_threshold_without_proving() {
    // days_elapsed == 365 makes CAGR ~= 100% (doubling); a threshold
    // placed exactly at 100% should straddle the tiny certified bracket.
    let input = GuestInput {
        claim: MetricClaim::Cagr {
            index_start: SCALE,
            index_end: SCALE * 2,
            days_elapsed: 365,
            operator: ClaimOperator::GreaterThan(SCALE), // > 100%
        },
    };
    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();
    let executor = default_executor();
    let session = executor
        .execute(env, GUEST_ELF)
        .expect("a valid claim must execute successfully");
    let output: GuestOutput = session
        .journal
        .decode()
        .expect("journal decodes to GuestOutput");
    assert_eq!(output.verdict, ClaimVerdict::IndeterminatePrecision);
}

/// Execution only: a claim clearly false against the bracket commits
/// `Rejected`.
#[test]
fn execution_rejects_a_clearly_false_claim_without_proving() {
    let input = GuestInput {
        claim: MetricClaim::Cagr {
            index_start: SCALE,
            index_end: SCALE * 2,
            days_elapsed: 365,
            operator: ClaimOperator::GreaterThan(SCALE * 10), // > 1000%
        },
    };
    let env = ExecutorEnv::builder()
        .write(&input)
        .unwrap()
        .build()
        .unwrap();
    let executor = default_executor();
    let session = executor
        .execute(env, GUEST_ELF)
        .expect("a valid claim must execute successfully");
    let output: GuestOutput = session
        .journal
        .decode()
        .expect("journal decodes to GuestOutput");
    assert_eq!(output.verdict, ClaimVerdict::Rejected);
}
