//! Builds the required symbol universe per the Binance adapter specification:
//! "Construir universo como união dos catálogos preservados no período,
//! símbolos observados em streams e históricos já conhecidos. **Não usar
//! somente posições atuais ou símbolos declarados pelo usuário.** Símbolo
//! negociado que depois zera continua obrigatório."
//!
//! [`SymbolUniverse`] is monotonic by construction: there is no method
//! that removes a symbol, and the only three ways to add one —
//! [`absorb_catalog_archive`](SymbolUniverse::absorb_catalog_archive),
//! [`absorb_stream`](SymbolUniverse::absorb_stream),
//! [`absorb_known_history`](SymbolUniverse::absorb_known_history) — are
//! exactly the three legitimate sources the spec names. There is
//! deliberately no constructor from "current positions" or a
//! caller-declared symbol list.

use crate::archive::CatalogArchive;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniverseSource {
    Catalog,
    Stream,
    KnownHistory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamObservation {
    pub symbol: String,
    pub observed_at_ms: u64,
}

pub struct SymbolUniverse {
    symbols: BTreeSet<String>,
    /// The source that first admitted each symbol, and when — kept for
    /// audit ("why is this symbol required?"), never consulted to decide
    /// membership; membership is `symbols.contains`, full stop.
    provenance: BTreeMap<String, (UniverseSource, u64)>,
}

impl Default for SymbolUniverse {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolUniverse {
    pub fn new() -> Self {
        Self {
            symbols: BTreeSet::new(),
            provenance: BTreeMap::new(),
        }
    }

    fn admit(&mut self, symbol: &str, source: UniverseSource, at_ms: u64) {
        if self.symbols.insert(symbol.to_string()) {
            self.provenance.insert(symbol.to_string(), (source, at_ms));
        }
        // Already present: provenance keeps the original admission,
        // consistent with "once required, always required" — a later
        // observation from a different source does not need to (and must
        // not) overwrite why the symbol first became required.
    }

    /// Unions every version ever archived (not just the latest) into the
    /// universe: a symbol delisted from the current catalog was still
    /// real while it was listed, and stays required.
    pub fn absorb_catalog_archive(&mut self, archive: &CatalogArchive) {
        for version in archive.versions() {
            for symbol in &version.symbols {
                self.admit(symbol, UniverseSource::Catalog, version.captured_at_ms);
            }
        }
    }

    /// Admits every symbol observed live in the user data stream — a
    /// symbol traded but never appearing in an archived catalog snapshot
    /// (e.g. captured between two catalog pulls) must still be required.
    pub fn absorb_stream(&mut self, observations: &[StreamObservation]) {
        for observation in observations {
            self.admit(
                &observation.symbol,
                UniverseSource::Stream,
                observation.observed_at_ms,
            );
        }
    }

    /// Admits symbols already known from prior periods/checkpoints for
    /// this track (e.g. carried over when resuming collection), at
    /// `known_at_ms`. This is how a symbol whose position has since gone
    /// to zero — and so would not appear in current positions — stays
    /// required without this module ever looking at "current positions"
    /// at all.
    pub fn absorb_known_history(
        &mut self,
        symbols: impl IntoIterator<Item = String>,
        known_at_ms: u64,
    ) {
        for symbol in symbols {
            self.admit(&symbol, UniverseSource::KnownHistory, known_at_ms);
        }
    }

    pub fn contains(&self, symbol: &str) -> bool {
        self.symbols.contains(symbol)
    }

    pub fn symbols(&self) -> &BTreeSet<String> {
        &self.symbols
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// The source and timestamp that first admitted `symbol`, if it is in
    /// the universe.
    pub fn provenance_of(&self, symbol: &str) -> Option<(UniverseSource, u64)> {
        self.provenance.get(symbol).copied()
    }
}
