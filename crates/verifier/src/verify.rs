//! Orchestrates the five checks this crate delivers (the acceptance criteria:
//! tamper/root/image/chain/coverage fail correctly) into one structured
//! report. Every input is supplied by the caller — nothing here reaches
//! out to a network — matching "CLI verifica sem API oficial."

use crate::anchor::{self, TrustedAnchor};
use crate::bundle::Bundle;
use crate::coverage;
use crate::receipt;
use crate::report::{CheckOutcome, Mode, VerificationReport};
use crate::root;
use crate::trust::{assess_origin, OriginAssessment, TrustList};
use checkpoint::{CalendarPolicy, Gap};
use std::collections::HashSet;

pub struct VerificationInput {
    pub bundle: Bundle,
    pub account_set_root: [u8; 32],
    pub member_ids: Vec<String>,
    pub receipt_bytes: Vec<u8>,
    pub expected_journal_bytes: Vec<u8>,
    pub image_id: [u32; 8],
    pub claimed_anchor: TrustedAnchor,
    pub trusted_anchors: HashSet<TrustedAnchor>,
    pub coverage_policy: CalendarPolicy,
    pub last_committed_end_ms: i64,
    pub now_ms: i64,
    pub claimed_gaps: Vec<Gap>,
    pub mode: Mode,
    /// Collectors the verifier trusts. Empty means every proof reads as
    /// self-attested.
    pub trust_list: TrustList,
}

pub fn verify(input: &VerificationInput) -> VerificationReport {
    let hash_mismatches = input.bundle.verify_hashes();
    let hashes = if hash_mismatches.is_empty() {
        CheckOutcome::Pass
    } else {
        CheckOutcome::Fail(format!(
            "{} file(s) failed hash verification: {:?}",
            hash_mismatches.len(),
            hash_mismatches
        ))
    };

    let account_set_root = if input.member_ids.is_empty() {
        CheckOutcome::Unavailable(
            "bundle carries no account membership to check accountSetRoot against".to_string(),
        )
    } else {
        match root::verify_account_set_root(&input.account_set_root, &input.member_ids) {
            Ok(()) => CheckOutcome::Pass,
            Err(error) => CheckOutcome::Fail(error.to_string()),
        }
    };

    let calculation = if input.receipt_bytes.is_empty() {
        CheckOutcome::Unavailable("bundle carries no receipt to verify".to_string())
    } else {
        match receipt::verify_receipt(
            &input.receipt_bytes,
            input.image_id,
            &input.expected_journal_bytes,
        ) {
            Ok(()) => CheckOutcome::Pass,
            Err(error) => CheckOutcome::Fail(error.to_string()),
        }
    };

    // The signer is read from the journal of a receipt that verified, and
    // only then: an unverified receipt's journal is just claimed bytes.
    let origin = if calculation.is_pass() {
        match receipt::decode_journal(&input.receipt_bytes) {
            Ok(journal) => assess_origin(journal.signer_fingerprint, journal.period_end_ms, &input.trust_list),
            Err(error) => OriginAssessment::Unavailable(error.to_string()),
        }
    } else {
        OriginAssessment::Unavailable("no verified receipt to read the signing collector from".to_string())
    };

    let registry = if input.claimed_anchor.tx_hash.is_empty() {
        CheckOutcome::Unavailable(
            "bundle carries no anchor to check against trusted anchors".to_string(),
        )
    } else {
        match anchor::verify_anchor(&input.claimed_anchor, &input.trusted_anchors) {
            Ok(()) => CheckOutcome::Pass,
            Err(error) => CheckOutcome::Fail(error.to_string()),
        }
    };

    let coverage = match coverage::verify_coverage(
        &input.coverage_policy,
        input.last_committed_end_ms,
        input.now_ms,
        &input.claimed_gaps,
    ) {
        Ok(()) => CheckOutcome::Pass,
        Err(mismatch) => CheckOutcome::Fail(format!(
            "recomputed {} gap(s) from the calendar, bundle claimed {}",
            mismatch.recomputed.len(),
            mismatch.claimed.len()
        )),
    };

    VerificationReport {
        mode: input.mode,
        hashes,
        account_set_root,
        calculation,
        registry,
        coverage,
        origin,
    }
}
