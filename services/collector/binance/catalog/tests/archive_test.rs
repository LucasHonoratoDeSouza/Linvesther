use binance_catalog::{CatalogArchive, CatalogError, CatalogSnapshot};
use std::collections::BTreeSet;

fn symbols(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|s| s.to_string()).collect()
}

#[test]
fn archives_versions_in_order() {
    let mut archive = CatalogArchive::new();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 100,
            symbols: symbols(&["BTCUSDT"]),
        })
        .unwrap();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 200,
            symbols: symbols(&["BTCUSDT", "ETHUSDT"]),
        })
        .unwrap();

    assert_eq!(archive.versions().len(), 2);
    assert_eq!(archive.latest().unwrap().captured_at_ms, 200);
}

#[test]
fn rejects_out_of_order_snapshot() {
    let mut archive = CatalogArchive::new();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 200,
            symbols: symbols(&["BTCUSDT"]),
        })
        .unwrap();

    let result = archive.archive(CatalogSnapshot {
        captured_at_ms: 100,
        symbols: symbols(&["ETHUSDT"]),
    });
    assert_eq!(
        result,
        Err(CatalogError::NotChronological {
            latest: 200,
            new: 100
        })
    );
}

#[test]
fn rejects_duplicate_timestamp() {
    let mut archive = CatalogArchive::new();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 200,
            symbols: symbols(&["BTCUSDT"]),
        })
        .unwrap();

    let result = archive.archive(CatalogSnapshot {
        captured_at_ms: 200,
        symbols: symbols(&["ETHUSDT"]),
    });
    assert_eq!(
        result,
        Err(CatalogError::NotChronological {
            latest: 200,
            new: 200
        })
    );
}

#[test]
fn has_version_at_or_before_matches_eligibility_check() {
    let mut archive = CatalogArchive::new();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 1_000,
            symbols: symbols(&["BTCUSDT"]),
        })
        .unwrap();

    assert!(archive.has_version_at_or_before(1_000));
    assert!(archive.has_version_at_or_before(2_000));
    assert!(!archive.has_version_at_or_before(999));
}

#[test]
fn no_versions_means_no_eligibility_coverage() {
    let archive = CatalogArchive::new();
    assert!(!archive.has_version_at_or_before(u64::MAX));
}

#[test]
fn delisted_symbols_are_those_present_earlier_but_absent_from_latest() {
    let mut archive = CatalogArchive::new();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 100,
            symbols: symbols(&["BTCUSDT", "LUNAUSDT"]),
        })
        .unwrap();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 200,
            symbols: symbols(&["BTCUSDT"]),
        })
        .unwrap();

    assert_eq!(archive.delisted_symbols(), symbols(&["LUNAUSDT"]));
}

#[test]
fn no_delisted_symbols_when_catalog_only_grows() {
    let mut archive = CatalogArchive::new();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 100,
            symbols: symbols(&["BTCUSDT"]),
        })
        .unwrap();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 200,
            symbols: symbols(&["BTCUSDT", "ETHUSDT"]),
        })
        .unwrap();

    assert!(archive.delisted_symbols().is_empty());
}

#[test]
fn all_symbols_ever_seen_unions_every_version() {
    let mut archive = CatalogArchive::new();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 100,
            symbols: symbols(&["BTCUSDT", "LUNAUSDT"]),
        })
        .unwrap();
    archive
        .archive(CatalogSnapshot {
            captured_at_ms: 200,
            symbols: symbols(&["BTCUSDT", "ETHUSDT"]),
        })
        .unwrap();

    assert_eq!(
        archive.all_symbols_ever_seen(),
        symbols(&["BTCUSDT", "ETHUSDT", "LUNAUSDT"])
    );
}
