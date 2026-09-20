//! Exercises the acceptance criteria: somente candles encerrados, política
//! fixa e idade ≤120s; preço ausente/futuro nunca vira zero.

use binance_prices::{
    mark_from_series, resolve_mark, select_candle, Candle, MarkError, PricePolicy,
};

fn candle(close_time_ms: u64, price: &str, is_closed: bool) -> Candle {
    Candle {
        symbol: "BTCUSDT".to_string(),
        open_time_ms: close_time_ms - 60_000,
        close_time_ms,
        close_price: price.to_string(),
        is_closed,
    }
}

#[test]
fn closed_recent_candle_resolves_a_mark() {
    let policy = PricePolicy::reference();
    let c = candle(1_000_000, "50000.00", true);
    let mark = resolve_mark("BTCUSDT", Some(&c), 1_010_000, &policy).unwrap();
    assert_eq!(mark.price, "50000.00");
    assert_eq!(mark.age_ms, 10_000);
}

#[test]
fn missing_candle_is_an_error_never_a_zero_price() {
    let policy = PricePolicy::reference();
    let result = resolve_mark("BTCUSDT", None, 1_000_000, &policy);
    assert_eq!(
        result,
        Err(MarkError::MissingPrice {
            symbol: "BTCUSDT".to_string(),
            evaluated_at_ms: 1_000_000
        })
    );
}

#[test]
fn unclosed_candle_is_rejected() {
    let policy = PricePolicy::reference();
    let c = candle(1_000_000, "50000.00", false);
    let result = resolve_mark("BTCUSDT", Some(&c), 1_000_000, &policy);
    assert_eq!(
        result,
        Err(MarkError::CandleNotClosed {
            symbol: "BTCUSDT".to_string()
        })
    );
}

#[test]
fn future_candle_is_rejected_never_used_to_mark() {
    let policy = PricePolicy::reference();
    let c = candle(2_000_000, "50000.00", true); // closes after evaluation instant
    let result = resolve_mark("BTCUSDT", Some(&c), 1_000_000, &policy);
    assert_eq!(
        result,
        Err(MarkError::CandleInTheFuture {
            symbol: "BTCUSDT".to_string(),
            evaluated_at_ms: 1_000_000,
            candle_close_time_ms: 2_000_000
        })
    );
}

#[test]
fn candle_exactly_at_the_age_limit_is_accepted() {
    let policy = PricePolicy::reference();
    let c = candle(1_000_000, "50000.00", true);
    let result = resolve_mark("BTCUSDT", Some(&c), 1_000_000 + 120_000, &policy);
    assert!(result.is_ok());
}

#[test]
fn candle_one_millisecond_past_the_age_limit_is_rejected() {
    let policy = PricePolicy::reference();
    let c = candle(1_000_000, "50000.00", true);
    let result = resolve_mark("BTCUSDT", Some(&c), 1_000_000 + 120_001, &policy);
    assert_eq!(
        result,
        Err(MarkError::CandleTooOld {
            symbol: "BTCUSDT".to_string(),
            age_ms: 120_001,
            max_age_ms: 120_000
        })
    );
}

#[test]
fn stale_candle_is_an_error_never_a_zero_price() {
    let policy = PricePolicy::reference();
    let c = candle(1_000_000, "50000.00", true);
    let result = resolve_mark("BTCUSDT", Some(&c), 10_000_000, &policy);
    assert!(matches!(result, Err(MarkError::CandleTooOld { .. })));
}

// --- candle selection from a series -----------------------------------

#[test]
fn selects_the_most_recent_closed_candle_not_the_absolute_latest() {
    let candles = vec![
        candle(900_000, "49000.00", true),
        candle(960_000, "49500.00", true),
        candle(1_020_000, "50500.00", false), // still open: must be ignored
    ];
    let selected = select_candle(&candles, 1_000_000).unwrap();
    assert_eq!(selected.close_time_ms, 960_000);
    assert_eq!(selected.close_price, "49500.00");
}

#[test]
fn ignores_future_closed_candles_when_selecting() {
    let candles = vec![
        candle(900_000, "49000.00", true),
        candle(1_500_000, "51000.00", true), // closed but in the future relative to eval time
    ];
    let selected = select_candle(&candles, 1_000_000).unwrap();
    assert_eq!(selected.close_time_ms, 900_000);
}

#[test]
fn empty_series_selects_nothing() {
    let candles: Vec<Candle> = vec![];
    assert!(select_candle(&candles, 1_000_000).is_none());
}

#[test]
fn series_with_only_unclosed_or_future_candles_selects_nothing() {
    let candles = vec![
        candle(1_000_000, "50000.00", false),
        candle(2_000_000, "51000.00", true),
    ];
    assert!(select_candle(&candles, 1_000_000).is_none());
}

#[test]
fn mark_from_series_end_to_end() {
    let policy = PricePolicy::reference();
    let candles = vec![
        candle(900_000, "49000.00", true),
        candle(960_000, "49500.00", true),
    ];
    let mark = mark_from_series("BTCUSDT", &candles, 1_000_000, &policy).unwrap();
    assert_eq!(mark.price, "49500.00");
}

#[test]
fn mark_from_series_with_no_qualifying_candle_is_an_error() {
    let policy = PricePolicy::reference();
    let candles = vec![candle(1_000_000, "50000.00", false)];
    let result = mark_from_series("BTCUSDT", &candles, 1_000_000, &policy);
    assert!(matches!(result, Err(MarkError::MissingPrice { .. })));
}
