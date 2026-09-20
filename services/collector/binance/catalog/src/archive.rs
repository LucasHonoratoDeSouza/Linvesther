//! Archives `exchangeInfo` snapshots per the Binance adapter specification:
//! "Arquivar versões desde antes de eligibleFrom e preservar delistados."
//!
//! A snapshot is a point-in-time symbol catalog; the archive is the
//! append-only, chronologically ordered history of every snapshot ever
//! captured. Nothing here ever removes a version — delisting a symbol
//! changes what a *later* snapshot contains, it does not edit or drop an
//! earlier one.

use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSnapshot {
    pub captured_at_ms: u64,
    pub symbols: BTreeSet<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CatalogError {
    #[error("snapshot captured_at_ms {new} is not after the latest archived version ({latest})")]
    NotChronological { latest: u64, new: u64 },
}

pub struct CatalogArchive {
    versions: Vec<CatalogSnapshot>,
}

impl Default for CatalogArchive {
    fn default() -> Self {
        Self::new()
    }
}

impl CatalogArchive {
    pub fn new() -> Self {
        Self {
            versions: Vec::new(),
        }
    }

    /// Appends `snapshot`. Rejects a snapshot whose `captured_at_ms` is
    /// not strictly after the latest archived version: catalog snapshots
    /// are captured in real time by the collector, so an out-of-order or
    /// duplicate-timestamp snapshot signals a bug upstream, not a valid
    /// historical fact to silently accept.
    pub fn archive(&mut self, snapshot: CatalogSnapshot) -> Result<(), CatalogError> {
        if let Some(latest) = self.versions.last() {
            if snapshot.captured_at_ms <= latest.captured_at_ms {
                return Err(CatalogError::NotChronological {
                    latest: latest.captured_at_ms,
                    new: snapshot.captured_at_ms,
                });
            }
        }
        self.versions.push(snapshot);
        Ok(())
    }

    pub fn versions(&self) -> &[CatalogSnapshot] {
        &self.versions
    }

    pub fn latest(&self) -> Option<&CatalogSnapshot> {
        self.versions.last()
    }

    /// True if some archived version was captured at or before
    /// `timestamp_ms` — the check the Binance adapter specification requires
    /// for `eligibleFrom` coverage: at least one catalog version dated
    /// before the track's eligibility window began.
    pub fn has_version_at_or_before(&self, timestamp_ms: u64) -> bool {
        self.versions
            .iter()
            .any(|v| v.captured_at_ms <= timestamp_ms)
    }

    /// The union of every symbol ever archived, across all versions —
    /// not just the latest. This is the raw material
    /// [`crate::universe::SymbolUniverse::absorb_catalog_archive`] draws
    /// from; exposed directly too since some callers only need this.
    pub fn all_symbols_ever_seen(&self) -> BTreeSet<String> {
        let mut all = BTreeSet::new();
        for version in &self.versions {
            all.extend(version.symbols.iter().cloned());
        }
        all
    }

    /// Symbols present in some earlier version but absent from the
    /// latest — i.e. delisted (or otherwise removed from the live
    /// catalog) as of the most recent snapshot. Empty if fewer than two
    /// versions are archived.
    pub fn delisted_symbols(&self) -> BTreeSet<String> {
        let Some(latest) = self.versions.last() else {
            return BTreeSet::new();
        };
        self.all_symbols_ever_seen()
            .difference(&latest.symbols)
            .cloned()
            .collect()
    }
}
