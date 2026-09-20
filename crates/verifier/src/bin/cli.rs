//! `linvesther-verify`: verifies a portable bundle entirely from
//! local files — no network call, matching the requirement that the CLI
//! verify without an official API. Per the protocol specification's verification algorithm
//! step 2 ("Obter trust manifests, políticas e image IDs de releases
//! confiáveis; não confiar apenas nos itens enviados pelo interessado"),
//! the trusted anchors and calendar policy are supplied by the verifier
//! operator (CLI flags), never read from the bundle itself; the image
//! ID is compiled into this binary from `zkvm-methods`, not
//! accepted as a bundle-supplied override either.
//!
//! `--offline` (the default; there is no online mode implemented yet)
//! reports `VALID_AS_OF(now)`, never a claim of current validity.

use checkpoint::{CalendarPolicy, Gap};
use clap::Parser;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use verifier::report::Mode;
use verifier::{verify, Bundle, Manifest, TrustList, TrustedAnchor, VerificationInput};

#[derive(Parser)]
struct Args {
    /// Directory containing manifest.json and the other bundle files.
    bundle_dir: PathBuf,
    /// JSON file of independently-obtained trusted anchors:
    /// `[{"txHash": "...", "blockNumber": 123}, ...]`.
    #[arg(long)]
    trusted_anchors: Option<PathBuf>,
    /// JSON list of collectors you trust (see `trust/collectors.json`). Without
    /// it no collector is trusted, so every proof reads as self-attested.
    #[arg(long)]
    trusted_collectors: Option<PathBuf>,
    /// Exit successfully even when the proof's data is self-attested. The
    /// origin is still printed; this only stops it from failing the run.
    #[arg(long)]
    accept_self_attested: bool,
    #[arg(long, default_value_t = 86_400_000)]
    interval_ms: i64,
    #[arg(long, default_value_t = 86_400_000)]
    grace_ms: i64,
}

#[derive(Deserialize)]
struct AccountsFile {
    #[serde(rename = "accountSetRoot")]
    account_set_root: String,
    members: Vec<String>,
}

#[derive(Deserialize)]
struct ProofEntry {
    #[serde(rename = "receiptHex")]
    receipt_hex: String,
    #[serde(rename = "journalHex")]
    journal_hex: String,
}

#[derive(Deserialize)]
struct AnchorEntry {
    #[serde(rename = "txHash")]
    tx_hash: String,
    #[serde(rename = "blockNumber")]
    block_number: u64,
}

#[derive(Deserialize)]
struct CheckpointsFile {
    #[serde(rename = "lastCommittedEndMs")]
    last_committed_end_ms: i64,
    gaps: Vec<GapEntry>,
}

#[derive(Deserialize)]
struct GapEntry {
    #[serde(rename = "startMs")]
    start_ms: i64,
    #[serde(rename = "endMs")]
    end_ms: i64,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_millis() as i64
}

fn hex_to_array32(hex_str: &str) -> [u8; 32] {
    let bytes = hex::decode(hex_str).expect("expected 64 hex chars");
    bytes.try_into().expect("expected exactly 32 bytes")
}

fn main() {
    let args = Args::parse();

    let manifest_path = args.bundle_dir.join("manifest.json");
    let manifest_json = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", manifest_path.display()));
    let manifest: Manifest =
        serde_json::from_str(&manifest_json).expect("manifest.json is not valid");

    let mut files = BTreeMap::new();
    for path in manifest.files.keys() {
        let full_path = args.bundle_dir.join(path);
        if let Ok(content) = fs::read_to_string(&full_path) {
            files.insert(path.clone(), content);
        }
    }
    let bundle = Bundle { manifest, files };

    let accounts: Option<AccountsFile> = bundle
        .file("accounts.json")
        .and_then(|content| serde_json::from_str(content).ok());
    let proofs: Option<Vec<ProofEntry>> = bundle
        .file("proofs/index.json")
        .and_then(|content| serde_json::from_str(content).ok());
    let anchors: Option<Vec<AnchorEntry>> = bundle
        .file("anchors/index.json")
        .and_then(|content| serde_json::from_str(content).ok());
    let checkpoints: Option<CheckpointsFile> = bundle
        .file("checkpoints/index.json")
        .and_then(|content| serde_json::from_str(content).ok());

    let trusted_anchors: HashSet<TrustedAnchor> = args
        .trusted_anchors
        .map(|path| {
            let content = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
            let entries: Vec<AnchorEntry> =
                serde_json::from_str(&content).expect("trusted anchors file is not valid");
            entries
                .into_iter()
                .map(|entry| TrustedAnchor {
                    tx_hash: entry.tx_hash,
                    block_number: entry.block_number,
                })
                .collect()
        })
        .unwrap_or_default();

    let trust_list = args
        .trusted_collectors
        .as_ref()
        .map(|path| {
            let content = fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
            TrustList::from_json(&content).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        })
        .unwrap_or_default();

    let input = VerificationInput {
        trust_list,
        account_set_root: accounts
            .as_ref()
            .map(|a| hex_to_array32(&a.account_set_root))
            .unwrap_or([0u8; 32]),
        member_ids: accounts.map(|a| a.members).unwrap_or_default(),
        receipt_bytes: proofs
            .as_ref()
            .and_then(|p| p.first())
            .map(|p| hex::decode(&p.receipt_hex).unwrap_or_default())
            .unwrap_or_default(),
        expected_journal_bytes: proofs
            .and_then(|p| p.into_iter().next())
            .map(|p| hex::decode(&p.journal_hex).unwrap_or_default())
            .unwrap_or_default(),
        image_id: verifier::guest_image_id(),
        claimed_anchor: anchors
            .as_ref()
            .and_then(|a| a.last())
            .map(|a| TrustedAnchor {
                tx_hash: a.tx_hash.clone(),
                block_number: a.block_number,
            })
            .unwrap_or(TrustedAnchor {
                tx_hash: String::new(),
                block_number: 0,
            }),
        trusted_anchors,
        coverage_policy: CalendarPolicy {
            interval_ms: args.interval_ms,
            grace_ms: args.grace_ms,
        },
        last_committed_end_ms: checkpoints
            .as_ref()
            .map(|c| c.last_committed_end_ms)
            .unwrap_or(0),
        now_ms: now_ms(),
        claimed_gaps: checkpoints
            .map(|c| {
                c.gaps
                    .into_iter()
                    .map(|g| Gap {
                        start_ms: g.start_ms,
                        end_ms: g.end_ms,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        bundle,
        mode: Mode::Offline { as_of_ms: now_ms() },
    };

    let report = verify(&input);
    println!("{report:#?}");
    println!("summary: {}", report.summary());
    println!("origin: {}", report.origin_summary());
    if !report.all_pass() {
        std::process::exit(1);
    }
    // A valid calculation over data nobody trusted is not a verified track
    // record, so it does not exit cleanly unless that was asked for.
    if !report.origin.is_trusted() && !args.accept_self_attested {
        eprintln!("the calculation verified, but the origin of the data is not a collector you trust; pass --accept-self-attested to accept it as such");
        std::process::exit(3);
    }
}
