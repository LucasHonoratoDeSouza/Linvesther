//! The explorer client against a stand-in for an explorer on a local port: every call is a
//! real HTTP request, answered as Etherscan and Blockscout answer.

mod common;

use common::{param, stand_in, StandIn};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wallet_client::{Endpoint, Explorer, ExplorerError};

fn explorer(server: &StandIn, window: usize, page_size: usize) -> Explorer {
    Explorer::new(Endpoint::Blockscout { base_url: server.url.clone(), api_key: None }).with_limits(window, page_size, Duration::ZERO, Duration::from_millis(5))
}

fn tx_row(block: u64, hash: &str) -> Value {
    json!({ "blockNumber": block.to_string(), "timeStamp": (1_700_000_000 + block).to_string(), "hash": hash, "from": "0xa", "to": "0xb", "value": "1", "gasUsed": "21000", "gasPrice": "1", "isError": "0" })
}

/// Rows of a stand-in chain, filtered and paged the way an explorer does.
fn paged(rows: &[Value], query: &str) -> (u16, String) {
    let start: u64 = param(query, "startblock").unwrap().parse().unwrap();
    let end: u64 = param(query, "endblock").unwrap().parse().unwrap();
    let page: usize = param(query, "page").unwrap().parse().unwrap();
    let offset: usize = param(query, "offset").unwrap().parse().unwrap();
    let matching: Vec<&Value> = rows.iter().filter(|r| {
        let b: u64 = r["blockNumber"].as_str().unwrap().parse().unwrap();
        b >= start && b <= end
    }).collect();
    let slice: Vec<&Value> = matching.into_iter().skip((page - 1) * offset).take(offset).collect();
    if slice.is_empty() {
        return (200, json!({ "status": "0", "message": "No transactions found", "result": [] }).to_string());
    }
    (200, json!({ "status": "1", "message": "OK", "result": slice }).to_string())
}

#[test]
fn a_list_longer_than_one_window_is_read_whole_with_no_row_lost_or_repeated() {
    // Blocks 1..=6, three rows in block 3 and two in block 4: with a window of 4, the first
    // window ends in the middle of block 4.
    let mut chain = vec![tx_row(1, "0x01"), tx_row(2, "0x02")];
    chain.extend(["0x31", "0x32", "0x33"].map(|h| tx_row(3, h)));
    chain.extend(["0x41", "0x42"].map(|h| tx_row(4, h)));
    chain.extend([tx_row(5, "0x05"), tx_row(6, "0x06")]);
    let rows = chain.clone();
    let server = stand_in(move |query| paged(&rows, query));
    let listing = explorer(&server, 4, 2).normal_txs("0xaddr", 0, 100).unwrap();
    let hashes: Vec<&str> = listing.rows.iter().map(|t| t.hash.as_str()).collect();
    assert_eq!(hashes, vec!["0x01", "0x02", "0x31", "0x32", "0x33", "0x41", "0x42", "0x05", "0x06"]);
    assert!(!listing.incomplete);
}

#[test]
fn a_block_that_alone_fills_a_window_is_refused_rather_than_read_in_part() {
    let chain: Vec<Value> = (0..6).map(|i| tx_row(7, &format!("0x{i}"))).collect();
    let server = stand_in(move |query| paged(&chain, query));
    match explorer(&server, 4, 2).normal_txs("0xaddr", 0, 100) {
        Err(ExplorerError::BlockTooLarge(7)) => {}
        other => panic!("expected BlockTooLarge(7), got {other:?}"),
    }
}

#[test]
fn an_empty_range_is_an_empty_whole_list_not_an_error() {
    let server = stand_in(|_| (200, json!({ "status": "0", "message": "No transactions found", "result": [] }).to_string()));
    let listing = explorer(&server, 10, 5).normal_txs("0xaddr", 0, 100).unwrap();
    assert!(listing.rows.is_empty() && !listing.incomplete);
}

#[test]
fn a_warning_that_rows_are_missing_is_carried_to_the_caller() {
    let server = stand_in(|_| (200, json!({ "status": "2", "message": "Some internal transactions within this block range have not yet been processed", "result": [
        { "blockNumber": "5", "timeStamp": "1700000005", "transactionHash": "0xab", "from": "0xa", "to": "0xb", "value": "9", "index": "1", "isError": "0" }
    ] }).to_string()));
    let listing = explorer(&server, 10, 5).internal_txs("0xaddr", 0, 100).unwrap();
    assert_eq!(listing.rows.len(), 1);
    assert!(listing.incomplete);
}

#[test]
fn a_rate_limit_answer_is_waited_out_and_retried_but_not_forever() {
    let calls = Arc::new(Mutex::new(0));
    let counter = calls.clone();
    let server = stand_in(move |_| {
        let mut n = counter.lock().unwrap();
        *n += 1;
        if *n <= 2 {
            (429, json!({ "message": "Too many requests", "result": null, "status": "0" }).to_string())
        } else {
            (200, json!({ "status": "1", "message": "OK", "result": "42" }).to_string())
        }
    });
    assert_eq!(explorer(&server, 10, 5).native_balance("0xaddr").unwrap(), "42");
    assert_eq!(*calls.lock().unwrap(), 3);

    let always = stand_in(|_| (429, "slow down".into()));
    assert!(matches!(explorer(&always, 10, 5).native_balance("0xaddr"), Err(ExplorerError::Answer(_))));
    assert_eq!(always.seen.lock().unwrap().len(), 4, "the first try and three retries");
}

#[test]
fn each_explorer_is_asked_in_its_own_way() {
    let server = stand_in(|query| match param(query, "action").as_deref() {
        Some("eth_block_number") => (200, json!({ "jsonrpc": "2.0", "id": 1, "result": "0x10" }).to_string()),
        Some("tokenlist") => (200, json!({ "status": "1", "message": "OK", "result": [{ "balance": "5", "contractAddress": "0xTok", "decimals": "6", "name": "T", "symbol": "TOK", "type": "ERC-20" }] }).to_string()),
        Some("balance") => (200, json!({ "status": "1", "message": "OK", "result": "7" }).to_string()),
        other => panic!("unexpected {other:?}"),
    });
    let blockscout = Explorer::new(Endpoint::Blockscout { base_url: server.url.clone(), api_key: Some("KEY".into()) }).with_limits(10, 5, Duration::ZERO, Duration::ZERO);
    assert_eq!(blockscout.head_block().unwrap(), 16);
    let tokens = blockscout.tokens("0xAddr").unwrap();
    assert_eq!((tokens[0].contract.as_str(), tokens[0].balance.as_str()), ("0xtok", "5"), "holdings come with their balances");
    assert_eq!(blockscout.native_balance("0xAddr").unwrap(), "7");
    let queries = server.seen.lock().unwrap().clone();
    assert!(queries.iter().all(|q| param(q, "apikey").as_deref() == Some("KEY")), "the key rides on every call: {queries:?}");
    assert!(queries.iter().any(|q| param(q, "module").as_deref() == Some("block")), "Blockscout's block number call: {queries:?}");
}
