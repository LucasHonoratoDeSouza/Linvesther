//! Readings of an address on a synthetic chain whose balances are computed independently of the
//! reader, so a reading that disagrees with the chain cannot be hidden by agreeing with itself.

use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use wallet_client::snapshot::{read_chain, unexplained, ReadRequest, SnapshotError};
use wallet_client::synthetic::{SyntheticChain, SyntheticPrices};
use wallet_client::wire::{InternalTx, NormalTx, TokenTransfer};
use wallet_client::Chain;

const ME: &str = "0xaaaa";
const OTHER: &str = "0xbbbb";
const ETH: u128 = 1_000_000_000_000_000_000;

fn ethereum() -> &'static Chain {
    Chain::by_id(1).unwrap()
}

fn d(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap()
}

fn receive(hash: &str, block: u64, wei: u128) -> NormalTx {
    NormalTx { hash: hash.into(), block, time_ms: block * 12_000, from: OTHER.into(), to: ME.into(), value: wei.to_string(), gas_used: "21000".into(), gas_price: "1000000000".into(), failed: false }
}

fn send(hash: &str, block: u64, wei: u128) -> NormalTx {
    NormalTx { hash: hash.into(), block, time_ms: block * 12_000, from: ME.into(), to: OTHER.into(), value: wei.to_string(), gas_used: "21000".into(), gas_price: "1000000000".into(), failed: false }
}

fn token_out(hash: &str, block: u64, contract: &str, raw: u128) -> TokenTransfer {
    TokenTransfer { hash: hash.into(), block, time_ms: block * 12_000, contract: contract.into(), symbol: "USDC".into(), decimals: 6, from: ME.into(), to: OTHER.into(), value: raw.to_string(), log_index: None }
}

fn read(chain: &SyntheticChain, prices: &SyntheticPrices, after: Option<u64>, known: &BTreeSet<String>) -> Result<Option<wallet_client::snapshot::Reading>, SnapshotError> {
    read_chain(chain, prices, &ReadRequest { chain: ethereum(), address: ME, after_block: after, known_assets: known })
}

fn decimals(asset: &str) -> u32 {
    if asset.ends_with(":native") { 18 } else { 6 }
}

#[test]
fn the_first_reading_is_the_balance_at_the_final_block_and_keeps_no_history_before_it() {
    let chain = SyntheticChain::new(1000);
    chain.open_with_native(5 * ETH);
    // Mined after the final block (968): part of the balance now, not of the balance then.
    chain.push_normal(receive("0xrecent", 990, ETH));
    let reading = read(&chain, &SyntheticPrices::new(), None, &BTreeSet::new()).unwrap().unwrap();
    assert_eq!((reading.head, reading.end), (1000, 968));
    assert_eq!(reading.anchor["ethereum:native"], d("5"));
    assert!(reading.confirmed.is_empty(), "history starts after the connection");
}

#[test]
fn a_later_reading_carries_what_became_final_and_its_balances_are_explained_to_the_wei() {
    let chain = SyntheticChain::new(1000);
    chain.open_with_native(5 * ETH);
    chain.push_normal(receive("0xin", 990, ETH));
    let first = read(&chain, &SyntheticPrices::new(), None, &BTreeSet::new()).unwrap().unwrap();

    chain.push_normal(send("0xout", 1010, ETH / 2));
    chain.set_head(1100);
    let second = read(&chain, &SyntheticPrices::new(), Some(first.end), &BTreeSet::new()).unwrap().unwrap();
    assert_eq!(second.end, 1068);
    assert_eq!(second.confirmed.iter().map(|e| e.tx_hash.as_str()).collect::<Vec<_>>(), vec!["0xin", "0xout", "0xout"], "the deposit, the send and its gas");
    // 5 + 1 - 0.5 - 21000 * 1 gwei = 5.499979
    assert_eq!(second.anchor["ethereum:native"], d("5.499979"));
    assert!(unexplained(&first.anchor, &second.confirmed, &second.anchor, &decimals).is_empty(), "the transfers explain the balances exactly");
}

#[test]
fn nothing_is_read_until_a_new_block_has_become_final() {
    let chain = SyntheticChain::new(1000);
    assert!(read(&chain, &SyntheticPrices::new(), Some(968), &BTreeSet::new()).unwrap().is_none());
    chain.set_head(1001);
    assert!(read(&chain, &SyntheticPrices::new(), Some(968), &BTreeSet::new()).unwrap().is_some());
}

#[test]
fn transfers_the_explorer_has_not_indexed_are_found_out_by_the_balances_not_hidden() {
    let chain = SyntheticChain::new(1000);
    chain.open_with_native(ETH);
    chain.open_with_token("0xusdc", "USDC", 6, 100_000_000);
    let prices = SyntheticPrices::new();
    prices.set("ethereum:0xusdc", d("1"));
    let first = read(&chain, &prices, None, &BTreeSet::new()).unwrap().unwrap();

    // A swap: 100 USDC out (indexed), 0.04 ether back from the router as an internal transfer.
    chain.push_token(token_out("0xswap", 1010, "0xusdc", 100_000_000));
    chain.push_internal(InternalTx { hash: "0xswap".into(), block: 1010, time_ms: 1010 * 12_000, from: OTHER.into(), to: ME.into(), value: (ETH / 25).to_string(), index: "1".into(), failed: false });
    chain.hide_internal_transfers(true);
    chain.set_head(1100);
    let known: BTreeSet<String> = first.anchor.keys().cloned().collect();
    let second = read(&chain, &prices, Some(first.end), &known).unwrap().unwrap();
    assert!(second.transfers_incomplete, "the explorer said it may be missing transfers");
    let missing = unexplained(&first.anchor, &second.confirmed, &second.anchor, &decimals);
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert_eq!(missing["ethereum:native"], d("0.04"), "the ether that came back with no transfer to explain it");
}

#[test]
fn a_reading_that_needs_blocks_the_explorer_has_not_reached_is_refused_not_guessed() {
    let chain = SyntheticChain::new(1000);
    chain.lag_indexer_by(100);
    match read(&chain, &SyntheticPrices::new(), None, &BTreeSet::new()) {
        Err(SnapshotError::ExplorerBehind { indexed: 900, needed: 968 }) => {}
        other => panic!("expected ExplorerBehind, got {other:?}"),
    }
}

#[test]
fn only_tokens_that_can_be_priced_or_were_touched_are_followed() {
    let chain = SyntheticChain::new(1000);
    chain.open_with_token("0xusdc", "USDC", 6, 100_000_000);
    chain.open_with_token("0xspam", "SPAM", 18, 5 * ETH);
    chain.open_with_token("0xold", "OLD", 6, 7_000_000);
    let prices = SyntheticPrices::new();
    prices.set("ethereum:0xusdc", d("1"));
    let known: BTreeSet<String> = ["ethereum:0xold".to_string()].into();
    let reading = read(&chain, &prices, None, &known).unwrap().unwrap();
    assert_eq!(reading.anchor["ethereum:0xusdc"], d("100"));
    assert!(!reading.anchor.contains_key("ethereum:0xspam"), "an unpriced token nobody touched is noise");
    assert_eq!(reading.ignored_unpriced, 1);
    assert_eq!(reading.anchor["ethereum:0xold"], d("7"), "an already-followed token stays followed even without a price");
}

#[test]
fn a_token_sold_out_after_the_last_reading_keeps_a_zero_balance_instead_of_vanishing() {
    let chain = SyntheticChain::new(1000);
    chain.open_with_token("0xusdc", "USDC", 6, 100_000_000);
    let prices = SyntheticPrices::new();
    prices.set("ethereum:0xusdc", d("1"));
    let first = read(&chain, &prices, None, &BTreeSet::new()).unwrap().unwrap();
    chain.push_token(token_out("0xsell", 1010, "0xusdc", 100_000_000));
    chain.set_head(1100);
    let known: BTreeSet<String> = first.anchor.keys().cloned().collect();
    let second = read(&chain, &prices, Some(first.end), &known).unwrap().unwrap();
    assert_eq!(second.anchor["ethereum:0xusdc"], Decimal::ZERO);
    assert!(unexplained(&first.anchor, &second.confirmed, &second.anchor, &decimals).is_empty());
}

#[test]
fn a_balance_too_large_to_hold_exactly_is_marked_unrepresentable() {
    let chain = SyntheticChain::new(1000);
    chain.open_with_token("0xhuge", "HUGE", 0, 10u128.pow(30));
    let known: BTreeSet<String> = ["ethereum:0xhuge".to_string()].into();
    let reading = read(&chain, &SyntheticPrices::new(), None, &known).unwrap().unwrap();
    assert!(reading.unrepresentable.contains("ethereum:0xhuge"));
    assert!(!reading.anchor.contains_key("ethereum:0xhuge"));
}

#[test]
fn rounding_dust_is_not_a_movement() {
    let previous: BTreeMap<String, Decimal> = [("ethereum:0xt".to_string(), d("1.000000"))].into();
    let anchor: BTreeMap<String, Decimal> = [("ethereum:0xt".to_string(), d("1.000005"))].into();
    assert!(unexplained(&previous, &[], &anchor, &decimals).is_empty(), "five millionths of a token with six decimals is under the tolerance");
    let anchor: BTreeMap<String, Decimal> = [("ethereum:0xt".to_string(), d("1.001"))].into();
    assert_eq!(unexplained(&previous, &[], &anchor, &decimals)["ethereum:0xt"], d("0.001"));
}
