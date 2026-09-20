//! Exercises the acceptance criteria: todas as páginas de cada símbolo são
//! lidas; mesmo ID em outro símbolo não colide; truncamento bloqueia
//! cobertura.

use binance_trades::{IngestError, PageOutcome, Trade, TradeStore};

fn trade(symbol: &str, id: u64, order_id: u64) -> Trade {
    Trade {
        symbol: symbol.to_string(),
        id,
        order_id,
        price: "100.00".to_string(),
        qty: "1.0".to_string(),
        commission: "0.001".to_string(),
        commission_asset: "BNB".to_string(),
        time_ms: 1_000 + id,
        is_buyer: true,
    }
}

#[test]
fn all_pages_of_a_symbol_are_recorded() {
    let mut store = TradeStore::new();
    store
        .ingest_page(
            "BTCUSDT",
            vec![trade("BTCUSDT", 1, 10), trade("BTCUSDT", 2, 11)],
            2,
        )
        .unwrap();
    store
        .ingest_page("BTCUSDT", vec![trade("BTCUSDT", 3, 12)], 2)
        .unwrap();

    assert_eq!(store.trades_for("BTCUSDT").len(), 3);
    assert_eq!(store.pages_fetched("BTCUSDT"), 2);
    assert_eq!(
        store.pagination_outcome("BTCUSDT"),
        Some(PageOutcome::Exhausted)
    );
}

#[test]
fn same_id_on_different_symbols_does_not_collide() {
    let mut store = TradeStore::new();
    store
        .ingest_page("BTCUSDT", vec![trade("BTCUSDT", 1, 10)], 10)
        .unwrap();
    store
        .ingest_page("ETHUSDT", vec![trade("ETHUSDT", 1, 20)], 10)
        .unwrap();

    let btc_trade = store.trade("BTCUSDT", 1).unwrap();
    let eth_trade = store.trade("ETHUSDT", 1).unwrap();
    assert_eq!(btc_trade.order_id, 10);
    assert_eq!(eth_trade.order_id, 20);
    assert_eq!(store.trades_for("BTCUSDT").len(), 1);
    assert_eq!(store.trades_for("ETHUSDT").len(), 1);
}

#[test]
fn truncated_page_blocks_coverage_until_a_short_page_is_seen() {
    let mut store = TradeStore::new();
    // Full page (capacity 2, returned 2): unproven.
    store
        .ingest_page(
            "BTCUSDT",
            vec![trade("BTCUSDT", 1, 10), trade("BTCUSDT", 2, 11)],
            2,
        )
        .unwrap();
    assert_eq!(
        store.pagination_outcome("BTCUSDT"),
        Some(PageOutcome::Unproven)
    );
    assert!(!store.is_symbol_exhausted("BTCUSDT"));
}

#[test]
fn endpoint_truncated_at_maximum_every_request_never_becomes_exhausted() {
    let mut store = TradeStore::new();
    for batch in 0..5u64 {
        let trades = vec![
            trade("BTCUSDT", batch * 1000, 1),
            trade("BTCUSDT", batch * 1000 + 1, 2),
        ];
        store.ingest_page("BTCUSDT", trades, 2).unwrap();
    }
    assert_eq!(store.pages_fetched("BTCUSDT"), 5);
    assert_eq!(
        store.pagination_outcome("BTCUSDT"),
        Some(PageOutcome::Unproven)
    );
    assert!(!store.is_symbol_exhausted("BTCUSDT"));
}

#[test]
fn empty_page_proves_exhaustion() {
    let mut store = TradeStore::new();
    store.ingest_page("BTCUSDT", vec![], 1000).unwrap();
    assert!(store.is_symbol_exhausted("BTCUSDT"));
}

#[test]
fn no_pages_fetched_means_not_exhausted() {
    let store = TradeStore::new();
    assert_eq!(store.pagination_outcome("BTCUSDT"), None);
    assert!(!store.is_symbol_exhausted("BTCUSDT"));
}

#[test]
fn ids_do_not_need_to_be_contiguous() {
    // IDs 5 and 9 belong to this account; the gap belongs to other
    // participants on the symbol, not a sign of missing data.
    let mut store = TradeStore::new();
    store
        .ingest_page(
            "BTCUSDT",
            vec![trade("BTCUSDT", 5, 1), trade("BTCUSDT", 9, 2)],
            10,
        )
        .unwrap();
    assert_eq!(store.trades_for("BTCUSDT").len(), 2);
    assert!(store.is_symbol_exhausted("BTCUSDT"));
}

// --- retry idempotency / conflict detection -----------------------------

#[test]
fn re_ingesting_the_identical_trade_is_idempotent() {
    let mut store = TradeStore::new();
    store
        .ingest_page("BTCUSDT", vec![trade("BTCUSDT", 1, 10)], 10)
        .unwrap();
    // A retry re-fetches the same page with identical content.
    store
        .ingest_page("BTCUSDT", vec![trade("BTCUSDT", 1, 10)], 10)
        .unwrap();

    assert_eq!(store.trades_for("BTCUSDT").len(), 1);
}

#[test]
fn conflicting_content_for_the_same_id_is_rejected_and_original_kept() {
    let mut store = TradeStore::new();
    store
        .ingest_page("BTCUSDT", vec![trade("BTCUSDT", 1, 10)], 10)
        .unwrap();

    let mut conflicting = trade("BTCUSDT", 1, 10);
    conflicting.qty = "999.0".to_string();
    let result = store.ingest_page("BTCUSDT", vec![conflicting], 10);

    assert_eq!(
        result,
        Err(IngestError::Conflict {
            symbol: "BTCUSDT".to_string(),
            id: 1
        })
    );
    assert_eq!(
        store.trade("BTCUSDT", 1).unwrap().qty,
        "1.0",
        "original must not be overwritten"
    );
}

#[test]
fn conflict_in_page_blocks_the_whole_page_not_a_partial_ingest() {
    let mut store = TradeStore::new();
    store
        .ingest_page("BTCUSDT", vec![trade("BTCUSDT", 1, 10)], 10)
        .unwrap();

    let mut conflicting = trade("BTCUSDT", 1, 10);
    conflicting.qty = "999.0".to_string();
    let page = vec![trade("BTCUSDT", 2, 11), conflicting];
    let result = store.ingest_page("BTCUSDT", page, 10);

    assert!(result.is_err());
    // trade id=2 from the same rejected page must not have been ingested.
    assert!(
        store.trade("BTCUSDT", 2).is_none(),
        "a conflicting page must not partially ingest"
    );
}
