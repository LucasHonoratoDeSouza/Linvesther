//! A block explorer's account API (Etherscan v2 or Blockscout, which answer in the same
//! shape), as far as this needs: an address's balances, holdings and its transfers between
//! two blocks. Calls are spaced and rate-limit answers waited out; a list is read whole or
//! not at all.

use crate::wire::{self, HeldToken, InternalTx, Listing, NormalTx, TokenTransfer, WireError};
use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Explorers answer at most 10,000 rows for one query, however many pages are asked for.
const DEFAULT_WINDOW: usize = 10_000;
const DEFAULT_PAGE_SIZE: usize = 1_000;
const RATE_LIMIT_RETRIES: u32 = 3;

/// Which explorer to ask, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// Etherscan's multi-chain API: one key, the network named in each request.
    EtherscanV2 { api_key: String, chain_id: u64 },
    /// A Blockscout instance for one network. A key is optional and raises its rate limit.
    Blockscout { base_url: String, api_key: Option<String> },
}

#[derive(Debug, thiserror::Error)]
pub enum ExplorerError {
    #[error("could not reach the explorer: {0}")]
    Network(String),
    #[error(transparent)]
    Answer(#[from] WireError),
    #[error("block {0} holds more rows than one query can return, so its transfers cannot be read whole")]
    BlockTooLarge(u64),
}

/// One explorer, for one network.
pub struct Explorer {
    endpoint: Endpoint,
    http: reqwest::blocking::Client,
    spacing: Duration,
    retry_wait: Duration,
    window: usize,
    page_size: usize,
    next_call: Mutex<Instant>,
}

impl Explorer {
    pub fn new(endpoint: Endpoint) -> Self {
        Explorer {
            endpoint,
            http: reqwest::blocking::Client::builder().user_agent("linvesther-collector").timeout(Duration::from_secs(30)).build().expect("the HTTP client can be built"),
            spacing: Duration::from_millis(250),
            retry_wait: Duration::from_secs(2),
            window: DEFAULT_WINDOW,
            page_size: DEFAULT_PAGE_SIZE,
            next_call: Mutex::new(Instant::now()),
        }
    }

    /// Smaller limits, for a test that cannot produce ten thousand rows.
    pub fn with_limits(mut self, window: usize, page_size: usize, spacing: Duration, retry_wait: Duration) -> Self {
        self.window = window;
        self.page_size = page_size;
        self.spacing = spacing;
        self.retry_wait = retry_wait;
        self
    }

    fn url(&self) -> String {
        match &self.endpoint {
            Endpoint::EtherscanV2 { .. } => "https://api.etherscan.io/v2/api".to_string(),
            Endpoint::Blockscout { base_url, .. } => format!("{}/api", base_url.trim_end_matches('/')),
        }
    }

    fn query(&self, module: &str, action: &str, extra: &[(&str, String)]) -> Vec<(String, String)> {
        let mut query = vec![("module".to_string(), module.to_string()), ("action".to_string(), action.to_string())];
        query.extend(extra.iter().map(|(k, v)| (k.to_string(), v.clone())));
        match &self.endpoint {
            Endpoint::EtherscanV2 { api_key, chain_id } => {
                query.push(("chainid".into(), chain_id.to_string()));
                query.push(("apikey".into(), api_key.clone()));
            }
            Endpoint::Blockscout { api_key: Some(key), .. } => query.push(("apikey".into(), key.clone())),
            Endpoint::Blockscout { api_key: None, .. } => {}
        }
        query
    }

    /// One spaced call, retried while the explorer says to slow down. Returns the parsed body.
    fn call(&self, module: &str, action: &str, extra: &[(&str, String)]) -> Result<Value, ExplorerError> {
        let query = self.query(module, action, extra);
        for attempt in 0..=RATE_LIMIT_RETRIES {
            let wait = {
                let mut next = self.next_call.lock().expect("the spacing lock is not poisoned");
                let slot = (*next).max(Instant::now());
                *next = slot + self.spacing;
                slot.saturating_duration_since(Instant::now())
            };
            std::thread::sleep(wait);
            let response = self.http.get(self.url()).query(&query).send().map_err(|e| ExplorerError::Network(e.without_url().to_string()))?;
            let status = response.status();
            let text = response.text().map_err(|e| ExplorerError::Network(e.without_url().to_string()))?;
            // A rate-limited answer may carry a JSON body or only a status.
            let body: Value = serde_json::from_str(&text).unwrap_or_else(|_| {
                if status.as_u16() == 429 {
                    serde_json::json!({ "status": "0", "message": "Too many requests", "result": null })
                } else {
                    serde_json::json!({ "status": "0", "message": format!("answered {status} with something that is not JSON"), "result": null })
                }
            });
            match wire::rate_limit_message(&body) {
                Some(_) if attempt < RATE_LIMIT_RETRIES => {
                    std::thread::sleep(self.retry_wait * (attempt + 1));
                }
                Some(said) => return Err(WireError::RateLimited(said).into()),
                None => return Ok(body),
            }
        }
        unreachable!("the loop returns on its last attempt")
    }

    /// The newest block the explorer knows.
    pub fn head_block(&self) -> Result<u64, ExplorerError> {
        let body = match self.endpoint {
            Endpoint::EtherscanV2 { .. } => self.call("proxy", "eth_blockNumber", &[])?,
            Endpoint::Blockscout { .. } => self.call("block", "eth_block_number", &[])?,
        };
        Ok(wire::parse_block_number(&body)?)
    }

    /// The tokens the address may hold: Blockscout lists its current holdings; Etherscan has no
    /// such call, so the tokens it ever received are found from its transfers. Balances are read
    /// separately, as of a block, from a node.
    pub fn tokens(&self, address: &str) -> Result<Vec<HeldToken>, ExplorerError> {
        match self.endpoint {
            Endpoint::Blockscout { .. } => {
                let body = self.call("account", "tokenlist", &[("address", address.to_string())])?;
                Ok(wire::parse_held_tokens(&body)?)
            }
            Endpoint::EtherscanV2 { .. } => {
                let head = self.head_block()?;
                let transfers = self.token_transfers(address, 0, head)?;
                let mut seen = std::collections::BTreeMap::new();
                for t in transfers.rows {
                    seen.entry(t.contract.clone()).or_insert(HeldToken { contract: t.contract, symbol: t.symbol, decimals: t.decimals, balance: String::new() });
                }
                Ok(seen.into_values().collect())
            }
        }
    }

    pub fn normal_txs(&self, address: &str, from: u64, to: u64) -> Result<Listing<NormalTx>, ExplorerError> {
        self.list("txlist", address, from, to, wire::parse_normal_txs, |t| t.block)
    }

    pub fn internal_txs(&self, address: &str, from: u64, to: u64) -> Result<Listing<InternalTx>, ExplorerError> {
        self.list("txlistinternal", address, from, to, wire::parse_internal_txs, |t| t.block)
    }

    pub fn token_transfers(&self, address: &str, from: u64, to: u64) -> Result<Listing<TokenTransfer>, ExplorerError> {
        self.list("tokentx", address, from, to, wire::parse_token_transfers, |t| t.block)
    }

    /// A list between two blocks (both included), oldest first, read whole. An explorer returns at
    /// most a window of rows per query, so when a window fills, the block it ended in — which may
    /// continue past it — is asked for on its own and the next window starts after it.
    fn list<T>(
        &self,
        action: &str,
        address: &str,
        from: u64,
        to: u64,
        parse: fn(&Value) -> Result<Listing<T>, WireError>,
        block_of: fn(&T) -> u64,
    ) -> Result<Listing<T>, ExplorerError> {
        let mut all: Vec<T> = Vec::new();
        let mut incomplete = false;
        let mut start = from;
        while start <= to {
            let (mut rows, window_full, warned) = self.window_of(action, address, start, to, parse)?;
            incomplete |= warned;
            if !window_full {
                all.append(&mut rows);
                break;
            }
            let last_block = rows.last().map(block_of).expect("a full window has rows");
            // A window that never leaves one block means that block alone cannot be read whole.
            if rows.first().map(block_of) == Some(last_block) {
                return Err(ExplorerError::BlockTooLarge(last_block));
            }
            rows.retain(|row| block_of(row) < last_block);
            all.append(&mut rows);
            let (mut block_rows, block_full, warned) = self.window_of(action, address, last_block, last_block, parse)?;
            incomplete |= warned;
            if block_full {
                return Err(ExplorerError::BlockTooLarge(last_block));
            }
            all.append(&mut block_rows);
            start = last_block + 1;
        }
        Ok(Listing { rows: all, incomplete })
    }

    /// Up to one window of rows from `start`, page by page; whether the window filled.
    #[allow(clippy::type_complexity)]
    fn window_of<T>(
        &self,
        action: &str,
        address: &str,
        start: u64,
        to: u64,
        parse: fn(&Value) -> Result<Listing<T>, WireError>,
    ) -> Result<(Vec<T>, bool, bool), ExplorerError> {
        let mut rows = Vec::new();
        let mut warned = false;
        let mut page = 1usize;
        loop {
            let body = self.call(
                "account",
                action,
                &[
                    ("address", address.to_string()),
                    ("startblock", start.to_string()),
                    ("endblock", to.to_string()),
                    ("page", page.to_string()),
                    ("offset", self.page_size.to_string()),
                    ("sort", "asc".into()),
                ],
            )?;
            let listing = parse(&body)?;
            warned |= listing.incomplete;
            let got = listing.rows.len();
            rows.extend(listing.rows);
            if got < self.page_size {
                return Ok((rows, false, warned));
            }
            if page * self.page_size >= self.window {
                return Ok((rows, true, warned));
            }
            page += 1;
        }
    }
}
