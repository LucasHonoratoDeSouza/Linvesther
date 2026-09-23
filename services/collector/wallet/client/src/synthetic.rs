//! A synthetic chain and price source for contract tests: they answer the questions
//! [`ChainReader`] and [`PriceProvider`] ask from data a test writes, and compute balances
//! independently of the code under test, so a reader that is wrong cannot agree with them by
//! accident. Not used by the collector itself.

use crate::prices::{Price, PriceError, PriceProvider};
use crate::reader::{ChainReader, ReadError};
use crate::wire::{HeldToken, InternalTx, Listing, NormalTx, TokenTransfer};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    head: u64,
    /// The head moves this far each time it is read, as a live chain does between calls.
    advance_per_head_read: u64,
    opening_native_wei: u128,
    /// contract -> (symbol, decimals, opening balance in the smallest unit)
    opening_tokens: BTreeMap<String, (String, u32, u128)>,
    normal: Vec<NormalTx>,
    internal: Vec<InternalTx>,
    tokens: Vec<TokenTransfer>,
    /// Report internal transfers as unavailable, the way an explorer that has not indexed them does.
    hide_internal: bool,
    /// How many blocks the explorer is behind the node.
    indexer_lag: u64,
}

/// A chain with one address of interest. Clones share the chain, so a test can keep a handle
/// while the code under test holds another.
#[derive(Clone)]
pub struct SyntheticChain {
    state: Arc<Mutex<State>>,
}

impl SyntheticChain {
    pub fn new(head: u64) -> Self {
        SyntheticChain { state: Arc::new(Mutex::new(State { head, ..State::default() })) }
    }

    pub fn set_head(&self, head: u64) {
        self.state.lock().unwrap().head = head;
    }

    pub fn advance_head_by_each_read(&self, blocks: u64) {
        self.state.lock().unwrap().advance_per_head_read = blocks;
    }

    pub fn open_with_native(&self, wei: u128) {
        self.state.lock().unwrap().opening_native_wei = wei;
    }

    pub fn open_with_token(&self, contract: &str, symbol: &str, decimals: u32, amount: u128) {
        self.state.lock().unwrap().opening_tokens.insert(contract.to_lowercase(), (symbol.to_string(), decimals, amount));
    }

    pub fn push_normal(&self, tx: NormalTx) {
        self.state.lock().unwrap().normal.push(tx);
    }

    pub fn push_internal(&self, tx: InternalTx) {
        self.state.lock().unwrap().internal.push(tx);
    }

    pub fn push_token(&self, transfer: TokenTransfer) {
        self.state.lock().unwrap().tokens.push(transfer);
    }

    pub fn hide_internal_transfers(&self, hide: bool) {
        self.state.lock().unwrap().hide_internal = hide;
    }

    pub fn lag_indexer_by(&self, blocks: u64) {
        self.state.lock().unwrap().indexer_lag = blocks;
    }

    fn native_at(state: &State, address: &str, head: u64) -> u128 {
        let mut wei = state.opening_native_wei as i128;
        for tx in state.normal.iter().filter(|t| t.block <= head) {
            let value: i128 = if tx.failed { 0 } else { tx.value.parse().unwrap() };
            if tx.to == address {
                wei += value;
            }
            if tx.from == address {
                wei -= value;
                wei -= tx.gas_used.parse::<i128>().unwrap() * tx.gas_price.parse::<i128>().unwrap();
            }
        }
        for tx in state.internal.iter().filter(|t| t.block <= head && !t.failed) {
            let value: i128 = tx.value.parse().unwrap();
            if tx.to == address {
                wei += value;
            }
            if tx.from == address {
                wei -= value;
            }
        }
        wei.max(0) as u128
    }

    fn token_at(state: &State, address: &str, contract: &str, head: u64) -> u128 {
        let mut amount = state.opening_tokens.get(contract).map_or(0, |t| t.2) as i128;
        for t in state.tokens.iter().filter(|t| t.block <= head && t.contract == contract) {
            let value: i128 = t.value.parse().unwrap();
            if t.to == address {
                amount += value;
            }
            if t.from == address {
                amount -= value;
            }
        }
        amount.max(0) as u128
    }
}

impl ChainReader for SyntheticChain {
    fn head_block(&self) -> Result<u64, ReadError> {
        let mut state = self.state.lock().unwrap();
        let head = state.head;
        state.head += state.advance_per_head_read;
        Ok(head)
    }

    fn indexed_block(&self) -> Result<u64, ReadError> {
        let state = self.state.lock().unwrap();
        Ok(state.head.saturating_sub(state.indexer_lag))
    }

    fn native_balance_at(&self, address: &str, block: u64) -> Result<String, ReadError> {
        let state = self.state.lock().unwrap();
        Ok(Self::native_at(&state, &address.to_lowercase(), block).to_string())
    }

    fn token_balances_at(&self, address: &str, contracts: &[String], block: u64) -> Result<Vec<Option<String>>, ReadError> {
        let state = self.state.lock().unwrap();
        let address = address.to_lowercase();
        Ok(contracts.iter().map(|c| Some(Self::token_at(&state, &address, &c.to_lowercase(), block).to_string())).collect())
    }

    fn tokens(&self, _address: &str) -> Result<Vec<HeldToken>, ReadError> {
        let state = self.state.lock().unwrap();
        let mut found: BTreeMap<String, (String, u32)> = state.opening_tokens.iter().map(|(c, t)| (c.clone(), (t.0.clone(), t.1))).collect();
        for t in state.tokens.iter().filter(|t| t.block <= state.head) {
            found.entry(t.contract.clone()).or_insert((t.symbol.clone(), t.decimals));
        }
        // Balances are not trusted from this call; a real explorer's may be stale.
        Ok(found.into_iter().map(|(contract, (symbol, decimals))| HeldToken { contract, symbol, decimals, balance: "0".into() }).collect())
    }

    fn normal_txs(&self, address: &str, from: u64, to: u64) -> Result<Listing<NormalTx>, ReadError> {
        let state = self.state.lock().unwrap();
        let address = address.to_lowercase();
        let rows = state.normal.iter().filter(|t| t.block >= from && t.block <= to && (t.from == address || t.to == address)).cloned().collect();
        Ok(Listing { rows, incomplete: false })
    }

    fn internal_txs(&self, address: &str, from: u64, to: u64) -> Result<Listing<InternalTx>, ReadError> {
        let state = self.state.lock().unwrap();
        if state.hide_internal {
            return Ok(Listing { rows: Vec::new(), incomplete: true });
        }
        let address = address.to_lowercase();
        let rows = state.internal.iter().filter(|t| t.block >= from && t.block <= to && (t.from == address || t.to == address)).cloned().collect();
        Ok(Listing { rows, incomplete: false })
    }

    fn token_transfers(&self, address: &str, from: u64, to: u64) -> Result<Listing<TokenTransfer>, ReadError> {
        let state = self.state.lock().unwrap();
        let address = address.to_lowercase();
        let rows = state.tokens.iter().filter(|t| t.block >= from && t.block <= to && (t.from == address || t.to == address)).cloned().collect();
        Ok(Listing { rows, incomplete: false })
    }
}

/// Prices a test writes down: a coin has a fixed price, or a price by moment, or none.
#[derive(Default)]
pub struct SyntheticPrices {
    fixed: Mutex<BTreeMap<String, Decimal>>,
}

impl SyntheticPrices {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, coin: &str, usd: Decimal) {
        self.fixed.lock().unwrap().insert(coin.to_string(), usd);
    }
}

impl PriceProvider for SyntheticPrices {
    fn current(&self, coins: &[String]) -> Result<BTreeMap<String, Price>, PriceError> {
        let fixed = self.fixed.lock().unwrap();
        Ok(coins.iter().filter_map(|c| fixed.get(c).map(|p| (c.clone(), Price { usd: *p, at_ms: 0 }))).collect())
    }

    fn at(&self, coins: &[String], at_ms: u64) -> Result<BTreeMap<String, Price>, PriceError> {
        let fixed = self.fixed.lock().unwrap();
        Ok(coins.iter().filter_map(|c| fixed.get(c).map(|p| (c.clone(), Price { usd: *p, at_ms }))).collect())
    }

    fn chart(&self, coin: &str, start_ms: u64, end_ms: u64, interval_ms: u64) -> Result<Vec<(u64, Decimal)>, PriceError> {
        let fixed = self.fixed.lock().unwrap();
        let Some(price) = fixed.get(coin) else { return Ok(Vec::new()) };
        let step = interval_ms.max(300_000);
        Ok((0..).map(|i| start_ms + i * step).take_while(|t| *t <= end_ms).map(|t| (t, *price)).collect())
    }
}
