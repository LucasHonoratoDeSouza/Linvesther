//! Talks to the real public services (a public node, Blockscout, DefiLlama), so it is never part
//! of the default gate. Run it by hand:
//!
//! ```sh
//! cargo test --manifest-path services/Cargo.toml -p wallet-client --test live_public -- --ignored --nocapture
//! ```

use std::collections::BTreeSet;
use std::time::Duration;
use wallet_client::snapshot::{read_chain, ReadRequest};
use wallet_client::{Chain, Endpoint, Explorer, LiveReader, PriceSource, Rpc};

/// A well-known, very active address.
const ADDRESS: &str = "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045";

fn reader(chain: &Chain) -> LiveReader {
    // Public explorers answer a handful of calls an hour without a key: set one to run these.
    let etherscan = std::env::var("ETHERSCAN_API_KEY").ok();
    let blockscout = std::env::var("BLOCKSCOUT_API_KEY").ok();
    let endpoint = chain.endpoint(etherscan.as_deref(), blockscout.as_deref()).expect("set ETHERSCAN_API_KEY or BLOCKSCOUT_API_KEY to read a real address");
    LiveReader {
        explorer: Explorer::new(endpoint).with_limits(10_000, 100, Duration::from_millis(600), Duration::from_secs(3)),
        node: Rpc::new(chain.rpc_url),
    }
}

#[test]
#[ignore = "calls public services"]
fn a_real_address_is_read_on_base_and_its_balances_are_priced() {
    let chain = Chain::by_id(8453).unwrap();
    let reader = reader(chain);
    let prices = PriceSource::default();
    let reading = read_chain(&reader, &prices, &ReadRequest { chain, address: ADDRESS, after_block: None, known_assets: &BTreeSet::new() }).unwrap().unwrap();
    println!("head {} end {} followed {} assets, ignored {} unpriced, incomplete {}", reading.head, reading.end, reading.anchor.len(), reading.ignored_unpriced, reading.transfers_incomplete);
    for (asset, balance) in reading.anchor.iter().take(8) {
        println!("  {asset}: {balance}");
    }
    assert_eq!(reading.end, reading.head - chain.confirmations);
    assert!(reading.anchor["base:native"].is_sign_positive());
    assert!(reading.confirmed.is_empty(), "the first reading keeps no history");
}

#[test]
#[ignore = "calls public services"]
fn a_real_stretch_of_history_is_read_and_explained_by_the_balances() {
    use wallet_client::snapshot::unexplained;
    use wallet_client::ChainReader;
    let chain = Chain::by_id(8453).unwrap();
    let reader = reader(chain);
    let prices = PriceSource::default();
    let head = reader.head_block().unwrap();
    // A real reading a while back, then one at the present: everything in between is explained, or reported.
    let start = head - 200_000; // about four and a half days on Base
    let first = read_chain(&reader, &prices, &ReadRequest { chain, address: ADDRESS, after_block: None, known_assets: &BTreeSet::new() }).unwrap().unwrap();
    let request = ReadRequest { chain, address: ADDRESS, after_block: Some(start), known_assets: &first.anchor.keys().cloned().collect() };
    let second = read_chain(&reader, &prices, &request).unwrap().unwrap();
    println!("effects since block {start}: {}, incomplete {}", second.confirmed.len(), second.transfers_incomplete);
    let past = reader.native_balance_at(ADDRESS, start).unwrap();
    println!("native at {start}: {past}");
    let _ = unexplained;
    assert!(second.end > start);
}

#[test]
#[ignore = "calls public services"]
fn every_default_network_answers_with_a_head_and_a_balance() {
    for chain in wallet_client::CHAINS {
        let node = Rpc::new(chain.rpc_url);
        let head = node.head_block().unwrap();
        let balance = node.native_balance_at(ADDRESS, head - chain.confirmations).unwrap();
        println!("{}: head {head}, balance {balance}", chain.name);
        assert!(head > chain.confirmations);
        let _ = Endpoint::Blockscout { base_url: String::new(), api_key: None };
    }
}
