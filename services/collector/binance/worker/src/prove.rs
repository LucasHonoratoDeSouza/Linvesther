//! Real ZK performance proof: takes this connection's real, collected
//! trades/flows, runs the same flow-adjusted reconstruction
//! `history.rs` uses for the dashboard, then proves the resulting
//! return series through `zkvm/methods/performance`'s real performance guest —
//! actual RISC Zero proving, not a stub. The result is a receipt that
//! anyone can verify against the guest's image ID without ever seeing
//! the account's raw trades, flows, balance or address.
//!
//! Crucially, the guest itself does **not** model external flows (see
//! its own README's "Out of scope") — its `reconcile()` is a strict
//! spot check that `sum(ledger_deltas) == nav.last() - nav.first()`.
//! Feeding it raw NAV for an account with real deposits/withdrawals
//! would silently reproduce the exact flow-adjustment bug `history.rs` already
//! caught and fixed once (a deposit read back as investment return).
//! This module avoids that by feeding the guest `history::HistoryResult
//! ::twr_index` — the chained index that already has deposits/
//! withdrawals factored out by `returns::compute_twr` *before* this
//! module ever sees it — as both `nav` and (via its own deltas)
//! `ledger_deltas`. The guest's spot reconciliation then holds
//! trivially (the synthetic series has no "external" activity of its
//! own left to reconcile), and what it proves is genuinely "this TWR
//! index evolved this way," not "this balance changed this way."
//!
//! `SourceEnvelope`/`GuestInput`/`GuestOutput` are duplicated here from
//! `zkvm/methods/performance/guest/src/{envelope,lib}.rs`, matching the
//! same cross-workspace pattern already used by
//! `zkvm/methods/performance/tests/guest_test.rs` — `services/` and
//! `zkvm/`'s `guest/` are separate Cargo workspaces (`guest/` targets
//! `riscv32im-risc0-zkvm-elf`), so the type cannot be shared directly;
//! `collector_attestation::envelope::SourceEnvelope` is used as-is here
//! (same `services/` workspace, no boundary), then wrapped into the
//! guest's wire-format mirror only for the actual `ExecutorEnv` write.

use crate::db;
use crate::history::{load_and_compute_history, HistoryError};
use binance_flows::NormalizedFlow;
use exchange_core::MarketData;
use std::sync::Arc;
use binance_trades::Trade;
use collector_attestation::envelope::SourceEnvelope;
use collector_attestation::signer::A0Signer;
use commitments::{data_commitment, generate_salt};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

const SCALE: i64 = 1_000_000;
const THIRTY_DAYS_MS: i64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Debug, thiserror::Error)]
pub enum ProveError {
    #[error(transparent)]
    History(#[from] HistoryError),
    #[error("need at least 2 checkpoints to prove a return, have {0}")]
    InsufficientHistory(usize),
    #[error(transparent)]
    Commitment(#[from] commitments::CommitmentError),
    #[error(transparent)]
    Salt(#[from] commitments::SaltError),
    #[error("invalid A0 signing key: {0}")]
    SigningKey(String),
    #[error("real proving failed: {0}")]
    Proving(String),
    #[error("receipt did not verify against its own image ID: {0}")]
    Verify(String),
    #[error("journal did not decode to the expected output: {0}")]
    Journal(String),
}

/// Wire-format mirror of `zkvm/methods/performance/guest/src/envelope.rs`
/// `SourceEnvelope` — field-for-field identical order/types to
/// `collector_attestation::envelope::SourceEnvelope`, so this struct is
/// used purely as the `ExecutorEnv` payload type; `origin::verify_origin`
/// inside the real guest recomputes the same digest from these same
/// fields, never trusting a precomputed one.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuestInputWire {
    envelope: SourceEnvelope,
    verifying_key_sec1: Vec<u8>,
    signature_compact: Vec<u8>,
    nav: Vec<i64>,
    ledger_deltas: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct GuestOutputWire {
    envelope_digest: [u8; 32],
    signer_fingerprint: [u8; 32],
    period_start_ms: i64,
    period_end_ms: i64,
    twr_index_scaled: i128,
    mdd_bp: i64,
    #[allow(dead_code)]
    capital: i64,
}

/// What a caller actually gets back: the claimed period and the two
/// real, guest-computed numbers (return and max drawdown) plus enough
/// to verify the receipt independently later. Never includes the raw
/// trades/flows, balance or account identifier — those stay behind the
/// envelope's commitments.
pub struct PerformanceProof {
    pub envelope_digest: [u8; 32],
    /// Fingerprint of the collector key that signed the source data, as
    /// committed in the proof's journal. A verifier compares it against
    /// its own list of trusted collectors.
    pub signer_fingerprint: [u8; 32],
    pub period_start_ms: i64,
    pub period_end_ms: i64,
    /// The proven return over the period, fixed-point scaled by
    /// `SCALE` (e.g. `42_000` at `SCALE = 1_000_000` means +4.2%).
    /// This is `twr_index_scaled - SCALE`, not raw NAV change.
    pub twr_return_scaled: i128,
    pub mdd_bp: i64,
    pub image_id: [u32; 8],
    pub receipt: risc0_zkvm::Receipt,
}

fn decimal_to_scaled_i64(value: Decimal) -> i64 {
    (value * Decimal::from(SCALE))
        .round()
        .to_i64()
        .unwrap_or(0)
}

fn trade_leaf(t: &Trade) -> serde_json::Value {
    json!({
        "symbol": t.symbol,
        "id": t.id,
        "orderId": t.order_id,
        "price": t.price,
        "qty": t.qty,
        "commission": t.commission,
        "commissionAsset": t.commission_asset,
        "timeMs": t.time_ms,
        "isBuyer": t.is_buyer,
    })
}

fn leg_json(leg: &binance_flows::AssetLeg) -> serde_json::Value {
    json!({ "asset": leg.asset, "amount": leg.amount, "isCredit": leg.is_credit })
}

fn flow_leaf(f: &NormalizedFlow) -> serde_json::Value {
    json!({
        "sourceNamespace": format!("{:?}", f.source_namespace),
        "sourceId": f.source_id,
        "economicTimeMs": f.economic_time_ms,
        "kind": f.kind,
        "legs": f.legs.iter().map(leg_json).collect::<Vec<_>>(),
        "fee": f.fee.as_ref().map(leg_json),
    })
}

fn sentinel_hash(label: &str) -> [u8; 32] {
    // No real trust-manifest/policy registry exists yet (see
    // zkvm/methods/performance/guest/README's "Out of scope") — this
    // hashes a fixed, documented canonical string rather than leaving
    // the field zeroed or absent, so it is still a real, reproducible
    // hash, just not yet backed by a registry a verifier can look up.
    Sha256::digest(label.as_bytes()).into()
}

/// Builds the signed envelope + fixed-point series for `connection_id`,
/// but does **not** run real proving — see `prove_performance` for
/// that. Split out so tests can exercise envelope construction (fast)
/// separately from real proving (slow, RAM-heavy).
async fn build_guest_input(
    pool: &PgPool,
    connection_id: Uuid,
    account_id: &str,
    client: Arc<dyn MarketData>,
    signing_key_bytes: &[u8; 32],
    now_ms: u64,
) -> Result<GuestInputWire, ProveError> {
    let trades = db::fetch_stored_trades(pool, connection_id)
        .await
        .map_err(HistoryError::from)?;
    let flows = db::fetch_stored_flows(pool, connection_id)
        .await
        .map_err(HistoryError::from)?;

    let history = load_and_compute_history(pool, connection_id, client, now_ms).await?;
    if history.twr_index.len() < 2 {
        return Err(ProveError::InsufficientHistory(history.twr_index.len()));
    }

    // The chained TWR index, not raw NAV — see this module's doc
    // comment for why that distinction is the whole point.
    let nav: Vec<i64> = history
        .twr_index
        .iter()
        .map(|p| decimal_to_scaled_i64(p.index))
        .collect();
    let ledger_deltas: Vec<i64> = nav.windows(2).map(|w| w[1] - w[0]).collect();

    let period_start_ms = history.twr_index.first().unwrap().time_ms as i64;
    let period_end_ms = history.twr_index.last().unwrap().time_ms as i64;

    let raw_evidence_root = data_commitment(
        &generate_salt()?,
        &json!({
            "trades": trades.iter().map(trade_leaf).collect::<Vec<_>>(),
            "flows": flows.iter().map(flow_leaf).collect::<Vec<_>>(),
        }),
    )?;
    let normalized_root = data_commitment(
        &generate_salt()?,
        &json!({
            "twrIndex": history
                .twr_index
                .iter()
                .map(|p| json!({ "timeMs": p.time_ms, "index": p.index.to_string() }))
                .collect::<Vec<_>>(),
        }),
    )?;
    let coverage_manifest_hash = data_commitment(
        &generate_salt()?,
        &json!({
            "periodStartMs": period_start_ms,
            "periodEndMs": period_end_ms,
            "checkpointCount": history.twr_index.len(),
        }),
    )?;
    let account_binding_commitment =
        data_commitment(&generate_salt()?, &json!({ "accountId": account_id }))?;
    let batch_commitment = data_commitment(
        &generate_salt()?,
        &json!({
            "tradeCount": trades.len(),
            "flowCount": flows.len(),
            "connectionId": connection_id.to_string(),
        }),
    )?;

    let envelope = SourceEnvelope {
        mechanism: "A0".to_string(),
        issuer_key_id: "binance-worker-collector".to_string(),
        trust_manifest_hash: sentinel_hash("LZK/trust-manifest/v1-sentinel/binance-worker"),
        account_binding_commitment,
        batch_commitment,
        raw_evidence_root,
        normalized_root,
        coverage_manifest_hash,
        period_start_ms,
        period_end_ms,
        observed_at_ms: now_ms as i64,
        expires_at_ms: now_ms as i64 + THIRTY_DAYS_MS,
        policy_hash: sentinel_hash("LZK/policy/v1-sentinel/spot-a0-collector-attested"),
        environment: "production".to_string(),
        source_session_binding: Sha256::digest(connection_id.as_bytes()).into(),
    };

    let signer = A0Signer::from_bytes(signing_key_bytes)
        .map_err(|e| ProveError::SigningKey(e.to_string()))?;
    let signature = signer.sign(&envelope);

    Ok(GuestInputWire {
        envelope,
        verifying_key_sec1: signer.verifying_key().to_sec1_bytes().to_vec(),
        signature_compact: signature.to_bytes().to_vec(),
        nav,
        ledger_deltas,
    })
}

/// Runs **real** RISC Zero proving (`risc0_zkvm::default_prover()`,
/// not dev-mode) over this connection's real, collected history. This
/// is CPU/RAM-heavy — the same class of operation that has frozen this
/// machine before during real proving; callers should warn before
/// invoking it, the same way `services/collector/binance/worker`'s own
/// README already flags the (much cheaper) historical reconstruction
/// as slow.
pub async fn prove_performance(
    pool: &PgPool,
    connection_id: Uuid,
    account_id: &str,
    client: Arc<dyn MarketData>,
    signing_key_bytes: &[u8; 32],
    now_ms: u64,
) -> Result<PerformanceProof, ProveError> {
    let input = build_guest_input(
        pool,
        connection_id,
        account_id,
        client,
        signing_key_bytes,
        now_ms,
    )
    .await?;

    let input_for_blocking = input.clone();
    let prove_info = tokio::task::spawn_blocking(move || {
        let env = risc0_zkvm::ExecutorEnv::builder()
            .write(&input_for_blocking)
            .map_err(|e| ProveError::Proving(e.to_string()))?
            .build()
            .map_err(|e| ProveError::Proving(e.to_string()))?;
        risc0_zkvm::default_prover()
            .prove(env, zkvm_methods::GUEST_ELF)
            .map_err(|e| ProveError::Proving(e.to_string()))
    })
    .await
    .map_err(|e| ProveError::Proving(e.to_string()))??;

    let receipt = prove_info.receipt;
    receipt
        .verify(zkvm_methods::GUEST_ID)
        .map_err(|e| ProveError::Verify(e.to_string()))?;

    let output: GuestOutputWire = receipt
        .journal
        .decode()
        .map_err(|e| ProveError::Journal(e.to_string()))?;

    Ok(PerformanceProof {
        envelope_digest: output.envelope_digest,
        signer_fingerprint: output.signer_fingerprint,
        period_start_ms: output.period_start_ms,
        period_end_ms: output.period_end_ms,
        twr_return_scaled: output.twr_index_scaled - SCALE as i128,
        mdd_bp: output.mdd_bp,
        image_id: zkvm_methods::GUEST_ID,
        receipt,
    })
}
