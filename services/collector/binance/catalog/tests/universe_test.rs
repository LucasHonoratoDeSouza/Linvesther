//! Exercises the acceptance criteria: catálogos históricos e stream alimentam
//! universo; símbolo zerado/delistado continua exigido.

use binance_catalog::{
    CatalogArchive, CatalogSnapshot, StreamObservation, SymbolUniverse, UniverseSource,
};
use std::collections::BTreeSet;

fn symbols(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|s| s.to_string()).collect()
}

#[test]
fn catalog_history_feeds_the_universe() {
    let mut archive = CatalogArchive::new();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 100,
            symbols: symbols(&["BTCUSDT", "ETHUSDT"]),
        })
        .unwrap();

    let mut universe = SymbolUniverse::new();
    universe.absorb_catalog_archive(&archive);

    assert!(universe.contains("BTCUSDT"));
    assert!(universe.contains("ETHUSDT"));
    assert_eq!(universe.len(), 2);
}

#[test]
fn stream_observations_feed_the_universe() {
    let mut universe = SymbolUniverse::new();
    universe.absorb_stream(&[StreamObservation {
        symbol: "SOLUSDT".to_string(),
        observed_at_ms: 500,
    }]);

    assert!(universe.contains("SOLUSDT"));
    assert_eq!(
        universe.provenance_of("SOLUSDT"),
        Some((UniverseSource::Stream, 500))
    );
}

#[test]
fn stream_admits_a_symbol_never_seen_in_any_catalog_snapshot() {
    // Represents a symbol traded between two catalog pulls: the catalog
    // alone would miss it, but the stream must not.
    let mut archive = CatalogArchive::new();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 100,
            symbols: symbols(&["BTCUSDT"]),
        })
        .unwrap();

    let mut universe = SymbolUniverse::new();
    universe.absorb_catalog_archive(&archive);
    universe.absorb_stream(&[StreamObservation {
        symbol: "NEWUSDT".to_string(),
        observed_at_ms: 150,
    }]);

    assert!(universe.contains("NEWUSDT"));
}

#[test]
fn known_history_feeds_the_universe() {
    let mut universe = SymbolUniverse::new();
    universe.absorb_known_history(["OLDUSDT".to_string()], 42);

    assert!(universe.contains("OLDUSDT"));
    assert_eq!(
        universe.provenance_of("OLDUSDT"),
        Some((UniverseSource::KnownHistory, 42))
    );
}

#[test]
fn delisted_symbol_stays_required_after_absorbing_the_full_archive() {
    let mut archive = CatalogArchive::new();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 100,
            symbols: symbols(&["BTCUSDT", "LUNAUSDT"]),
        })
        .unwrap();
    // LUNAUSDT delisted by the time of the second snapshot.
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 200,
            symbols: symbols(&["BTCUSDT"]),
        })
        .unwrap();

    let mut universe = SymbolUniverse::new();
    universe.absorb_catalog_archive(&archive);

    assert!(
        universe.contains("LUNAUSDT"),
        "a delisted symbol must stay in the universe"
    );
    assert!(universe.contains("BTCUSDT"));
}

#[test]
fn zeroed_position_symbol_stays_required_via_known_history() {
    // A symbol whose position later went to zero would not appear in
    // "current positions" — which this module never looks at. It stays
    // required because it was admitted via known-history (or catalog/
    // stream) at some point, and nothing here ever removes a symbol.
    let mut universe = SymbolUniverse::new();
    universe.absorb_known_history(["ZEROEDUSDT".to_string()], 10);

    // Simulate further updates that do NOT re-mention the zeroed symbol.
    let mut archive = CatalogArchive::new();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 100,
            symbols: symbols(&["BTCUSDT"]),
        })
        .unwrap();
    universe.absorb_catalog_archive(&archive);
    universe.absorb_stream(&[StreamObservation {
        symbol: "ETHUSDT".to_string(),
        observed_at_ms: 200,
    }]);

    assert!(
        universe.contains("ZEROEDUSDT"),
        "a zeroed-position symbol must remain required"
    );
    assert_eq!(universe.len(), 3);
}

#[test]
fn universe_never_shrinks_across_absorptions() {
    let mut universe = SymbolUniverse::new();
    let mut archive = CatalogArchive::new();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 100,
            symbols: symbols(&["BTCUSDT", "LUNAUSDT", "ETHUSDT"]),
        })
        .unwrap();
    universe.absorb_catalog_archive(&archive);
    let count_after_first = universe.len();

    // A later snapshot with fewer symbols (delisting) must not shrink
    // the universe when absorbed.
    let mut later_archive = CatalogArchive::new();
    later_archive
        .archive(CatalogSnapshot {
            captured_at_ms: 200,
            symbols: symbols(&["BTCUSDT"]),
        })
        .unwrap();
    universe.absorb_catalog_archive(&later_archive);

    assert!(universe.len() >= count_after_first);
    assert!(universe.contains("LUNAUSDT"));
    assert!(universe.contains("ETHUSDT"));
}

#[test]
fn re_admitting_from_a_different_source_does_not_overwrite_original_provenance() {
    let mut universe = SymbolUniverse::new();
    universe.absorb_known_history(["BTCUSDT".to_string()], 10);
    universe.absorb_stream(&[StreamObservation {
        symbol: "BTCUSDT".to_string(),
        observed_at_ms: 999,
    }]);

    assert_eq!(
        universe.provenance_of("BTCUSDT"),
        Some((UniverseSource::KnownHistory, 10))
    );
}

#[test]
fn empty_universe_is_empty() {
    let universe = SymbolUniverse::new();
    assert!(universe.is_empty());
    assert_eq!(universe.len(), 0);
}
