use metrics::{compute_win_rate, Fill, WinRateError};
use rust_decimal::Decimal;
use std::str::FromStr;

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn fill(time_ms: u64, qty: &str, price: &str, is_buy: bool) -> Fill {
    Fill {
        time_ms,
        qty: dec(qty),
        price: dec(price),
        is_buy,
    }
}

#[test]
fn a_single_profitable_round_trip_is_a_100_percent_win_rate() {
    let fills = vec![fill(1, "1", "100", true), fill(2, "1", "150", false)];
    let result = compute_win_rate(&fills).unwrap();
    assert_eq!(result.closed_round_trips, 1);
    assert_eq!(result.wins, 1);
    assert_eq!(result.win_rate, Decimal::ONE);
}

#[test]
fn a_single_losing_round_trip_is_a_0_percent_win_rate() {
    let fills = vec![fill(1, "1", "100", true), fill(2, "1", "80", false)];
    let result = compute_win_rate(&fills).unwrap();
    assert_eq!(result.closed_round_trips, 1);
    assert_eq!(result.wins, 0);
    assert_eq!(result.win_rate, Decimal::ZERO);
}

#[test]
fn a_breakeven_round_trip_is_not_a_win_but_still_closes() {
    let fills = vec![fill(1, "1", "100", true), fill(2, "1", "100", false)];
    let result = compute_win_rate(&fills).unwrap();
    assert_eq!(result.closed_round_trips, 1);
    assert_eq!(result.wins, 0);
    assert_eq!(result.win_rate, Decimal::ZERO);
}

#[test]
fn two_wins_and_one_loss_is_a_two_thirds_win_rate() {
    let fills = vec![
        fill(1, "1", "100", true),
        fill(2, "1", "110", false), // win
        fill(3, "1", "100", true),
        fill(4, "1", "90", false), // loss
        fill(5, "1", "100", true),
        fill(6, "1", "120", false), // win
    ];
    let result = compute_win_rate(&fills).unwrap();
    assert_eq!(result.closed_round_trips, 3);
    assert_eq!(result.wins, 2);
    assert_eq!(result.win_rate, Decimal::from(2) / Decimal::from(3));
}

#[test]
fn fifo_matches_the_oldest_lot_first_across_a_partial_sell() {
    // Buy 1 @ 100, buy 1 @ 200, sell 1 (should match the FIRST lot at
    // 100, not the second) at 150 -> a win, not a loss.
    let fills = vec![
        fill(1, "1", "100", true),
        fill(2, "1", "200", true),
        fill(3, "1", "150", false),
    ];
    let result = compute_win_rate(&fills).unwrap();
    assert_eq!(result.closed_round_trips, 1);
    assert_eq!(
        result.wins, 1,
        "FIFO must match the 100-cost lot first, making this a win"
    );
}

#[test]
fn a_sell_spanning_two_lots_closes_two_separate_round_trips() {
    let fills = vec![
        fill(1, "1", "100", true),  // lot A: win at 150
        fill(2, "1", "200", true),  // lot B: loss at 150
        fill(3, "2", "150", false), // consumes both lots
    ];
    let result = compute_win_rate(&fills).unwrap();
    assert_eq!(result.closed_round_trips, 2);
    assert_eq!(result.wins, 1);
    assert_eq!(result.win_rate, Decimal::from(1) / Decimal::from(2));
}

#[test]
fn an_unmatched_sell_with_no_open_lot_is_skipped_not_fabricated() {
    let fills = vec![fill(1, "1", "100", false)];
    assert_eq!(
        compute_win_rate(&fills).unwrap_err(),
        WinRateError::NoClosedRoundTrips
    );
}

#[test]
fn no_fills_at_all_reports_no_closed_round_trips_rather_than_zero() {
    assert_eq!(
        compute_win_rate(&[]).unwrap_err(),
        WinRateError::NoClosedRoundTrips
    );
}

#[test]
fn only_open_buys_with_no_sells_reports_no_closed_round_trips() {
    let fills = vec![fill(1, "1", "100", true)];
    assert_eq!(
        compute_win_rate(&fills).unwrap_err(),
        WinRateError::NoClosedRoundTrips
    );
}
