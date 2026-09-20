//! Evidence types the coverage engine reasons about.
//!
//! These mirror the endpoint families enumerated in the Binance adapter specification
//! at the granularity needed to decide `POLICY_COMPLETE` vs `INCOMPLETE`; they
//! are not a full transcription of Binance response schemas.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Effective permissions of the API key, as reported by `apiRestrictions`.
///
/// New or unrecognized flags are carried in `unknown_flags` rather than
/// dropped, so the engine can fail closed on capabilities it does not yet
/// classify.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permissions {
    pub enable_reading: bool,
    pub enable_withdrawals: bool,
    pub enable_internal_transfer: bool,
    pub enable_spot_and_margin_trading: bool,
    pub enable_margin: bool,
    pub enable_futures: bool,
    pub unknown_flags: Vec<String>,
}

impl Permissions {
    /// A0 requires a strictly read-only key: only `enable_reading` may be
    /// true, and no unrecognized flag may be present.
    pub fn is_read_only(&self) -> bool {
        self.enable_reading
            && !self.enable_withdrawals
            && !self.enable_internal_transfer
            && !self.enable_spot_and_margin_trading
            && !self.enable_margin
            && !self.enable_futures
            && self.unknown_flags.is_empty()
    }
}

/// Rotation of the API key/session bound to a stable account UID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationEvent {
    pub observed_at_ms: i64,
    pub previous_key_id: String,
    pub next_key_id: String,
}

/// Identity binding for the collected account: UID never changes across
/// key rotations; a different UID is a different account/track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBinding {
    pub uid: String,
    pub environment: Environment,
    pub perimeter: String,
    pub rotations: Vec<RotationEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Environment {
    Production,
    Test,
}

/// Outcome of paginating one bounded query against an authenticated endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageOutcome {
    /// The query returned fewer records than the page capacity, or an
    /// explicit end-of-data cursor: no further records can exist.
    Exhausted,
    /// The page was full and no safe cursor/boundary was available to prove
    /// the next page would not skip records.
    Unproven,
}

/// Coverage of `myTrades` for a single symbol across the observation window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolTradeCoverage {
    pub symbol: String,
    pub pages_fetched: u32,
    pub outcome: PageOutcome,
    /// Distinct fee currencies observed (e.g. `BNB` when fee discount is
    /// active); recorded for evidence, not required to equal quote asset.
    pub fee_currencies_observed: BTreeSet<String>,
}

/// A family of balance-affecting endpoints (deposits, withdrawals,
/// universal transfers, Convert, dust, dividends).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowFamilyCoverage {
    pub name: String,
    pub outcome: PageOutcome,
    /// Status/type values actually observed (e.g. `COMPLETED`, `FAILED`,
    /// `REVERSED` for withdrawals).
    pub kinds_observed: BTreeSet<String>,
    /// Status/type values the collector does not yet classify; any entry
    /// here fails the family closed regardless of `outcome`.
    pub unknown_kinds: BTreeSet<String>,
}

impl FlowFamilyCoverage {
    pub fn is_complete(&self) -> bool {
        self.outcome == PageOutcome::Exhausted && self.unknown_kinds.is_empty()
    }
}

/// One archived snapshot of `exchangeInfo`, keyed by the symbols it lists.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogVersion {
    pub captured_at_ms: i64,
    pub symbols: BTreeSet<String>,
}

/// Reconciliation of a held asset's ledger deltas against the observed
/// balance at the cut.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetReconciliation {
    pub asset: String,
    pub ledger_total: BalanceComponents,
    pub observed_total: BalanceComponents,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BalanceComponents {
    pub free: f64,
    pub locked: f64,
}

impl BalanceComponents {
    pub fn total(&self) -> f64 {
        self.free + self.locked
    }

    pub fn reconciles_with(&self, other: &BalanceComponents, epsilon: f64) -> bool {
        (self.total() - other.total()).abs() <= epsilon
    }
}

/// All evidence collected for one attempted B0 homologation window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub binding: AccountBinding,
    pub permissions: Permissions,
    /// Union of catalog symbols, stream-observed symbols and previously
    /// known symbols; monotonic across windows (never shrinks).
    pub symbol_universe: BTreeSet<String>,
    pub eligible_from_ms: i64,
    pub trades: BTreeMap<String, SymbolTradeCoverage>,
    pub deposits: FlowFamilyCoverage,
    pub withdrawals: FlowFamilyCoverage,
    pub transfers: FlowFamilyCoverage,
    pub convert: FlowFamilyCoverage,
    pub dust: FlowFamilyCoverage,
    pub dividends: FlowFamilyCoverage,
    pub catalog_versions: Vec<CatalogVersion>,
    pub reconciliations: Vec<AssetReconciliation>,
}
