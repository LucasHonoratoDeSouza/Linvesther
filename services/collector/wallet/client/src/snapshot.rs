//! One reading of an address on one network: the transfers since the last reading up to a block
//! old enough to be final, and the balances as of that same block. Reading balances at the block
//! itself (rather than adding up recent transfers) keeps the two exactly consistent, so what the
//! transfers do not explain shows up as a difference to be accounted for, never as an error that
//! creeps into the history.

use crate::amounts::{units, NATIVE_DECIMALS};
use crate::chains::Chain;
use crate::effects::{effects_of, Effect};
use crate::prices::{PriceError, PriceProvider};
use crate::reader::{ChainReader, ReadError};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};

/// A balance difference this small (in smallest units) is rounding, not a movement.
const DUST_UNITS: u32 = 10;

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error("could not price tokens: {0}")]
    Price(#[from] PriceError),
    #[error("the explorer has indexed only to block {indexed}, and the reading needs block {needed}")]
    ExplorerBehind { indexed: u64, needed: u64 },
}

/// What is known about a token beyond its address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenMeta {
    pub contract: String,
    pub symbol: String,
    pub decimals: u32,
}

pub struct ReadRequest<'a> {
    pub chain: &'static Chain,
    pub address: &'a str,
    /// The newest block whose transfers are already kept; none on the first reading.
    pub after_block: Option<u64>,
    /// Assets already followed for this address: their balance is always read, priced or not.
    pub known_assets: &'a BTreeSet<String>,
}

#[derive(Debug)]
pub struct Reading {
    pub head: u64,
    /// The block the reading is as of.
    pub end: u64,
    /// Effects of the blocks after the last reading, up to `end`.
    pub confirmed: Vec<Effect>,
    /// The balance of each followed asset as of `end`, zero included.
    pub anchor: BTreeMap<String, Decimal>,
    /// Token assets and what is known about each.
    pub tokens: BTreeMap<String, TokenMeta>,
    /// The explorer warned that some transfers of this range may be missing.
    pub transfers_incomplete: bool,
    /// Assets whose amount cannot be represented exactly.
    pub unrepresentable: BTreeSet<String>,
    /// Tokens held that have no price and were never touched, so they are not followed.
    pub ignored_unpriced: usize,
}

/// The reading as of the newest final block, or `None` when no new block has become final since
/// `after_block`.
pub fn read_chain(reader: &dyn ChainReader, prices: &dyn PriceProvider, request: &ReadRequest) -> Result<Option<Reading>, SnapshotError> {
    let chain = request.chain;
    let address = request.address.to_lowercase();
    let head = reader.head_block()?;
    let end = head.saturating_sub(chain.confirmations);
    if request.after_block.is_some_and(|after| end <= after) {
        return Ok(None);
    }
    let indexed = reader.indexed_block()?;
    if indexed < end {
        return Err(SnapshotError::ExplorerBehind { indexed, needed: end });
    }
    let from = request.after_block.map_or(end + 1, |after| after + 1);

    let (normal, internal, tokens_in_range) = if from <= end {
        (reader.normal_txs(&address, from, end)?, reader.internal_txs(&address, from, end)?, reader.token_transfers(&address, from, end)?)
    } else {
        Default::default()
    };
    let transfers_incomplete = normal.incomplete || internal.incomplete || tokens_in_range.incomplete;
    let effects = effects_of(chain, &address, &normal.rows, &internal.rows, &tokens_in_range.rows);

    // Tokens received after `end` are not in the effects yet but will be, so their balance at
    // `end` (zero, or what came before) has to be followed from now.
    let recent_tokens = if end < head { reader.token_transfers(&address, end + 1, head)?.rows } else { Vec::new() };

    let mut meta: BTreeMap<String, TokenMeta> = BTreeMap::new();
    let mut must_follow: BTreeSet<String> = request.known_assets.clone();
    for effect in &effects.list {
        must_follow.insert(effect.asset.clone());
    }
    for transfer in tokens_in_range.rows.iter().chain(recent_tokens.iter()) {
        let asset = chain.token_asset(&transfer.contract);
        must_follow.insert(asset.clone());
        meta.entry(asset).or_insert_with(|| TokenMeta { contract: transfer.contract.clone(), symbol: transfer.symbol.clone(), decimals: transfer.decimals });
    }
    let listed = reader.tokens(&address)?;
    for token in &listed {
        meta.entry(chain.token_asset(&token.contract)).or_insert_with(|| TokenMeta { contract: token.contract.clone(), symbol: token.symbol.clone(), decimals: token.decimals });
    }

    // A token nobody touched is followed only if it can be priced: the rest is unsolicited noise.
    let unfollowed: Vec<(String, String)> = meta.iter().filter(|(asset, _)| !must_follow.contains(*asset)).map(|(asset, m)| (asset.clone(), chain.token_price_id(&m.contract))).collect();
    let priced: BTreeSet<String> = if unfollowed.is_empty() { BTreeSet::new() } else { prices.current(&unfollowed.iter().map(|(_, id)| id.clone()).collect::<Vec<_>>())?.into_keys().collect() };
    let mut ignored_unpriced = 0;
    let mut followed: Vec<String> = Vec::new();
    for (asset, m) in &meta {
        if must_follow.contains(asset) || priced.contains(&chain.token_price_id(&m.contract)) {
            followed.push(asset.clone());
        } else {
            ignored_unpriced += 1;
        }
    }
    meta.retain(|asset, _| followed.contains(asset));

    let mut anchor = BTreeMap::new();
    let mut unrepresentable = effects.unrepresentable.clone();
    let native = chain.native_asset();
    match units(&reader.native_balance_at(&address, end)?, NATIVE_DECIMALS) {
        Some(balance) => {
            anchor.insert(native, balance);
        }
        None => {
            unrepresentable.insert(native);
        }
    }
    let contracts: Vec<String> = followed.iter().filter_map(|asset| meta.get(asset)).map(|m| m.contract.clone()).collect();
    let balances = if contracts.is_empty() { Vec::new() } else { reader.token_balances_at(&address, &contracts, end)? };
    for (asset, raw) in followed.iter().zip(balances) {
        let decimals = meta[asset].decimals;
        match raw.as_deref().and_then(|raw| units(raw, decimals)) {
            Some(balance) => {
                anchor.insert(asset.clone(), balance);
            }
            None => {
                unrepresentable.insert(asset.clone());
            }
        }
    }
    // An asset followed only because it has effects but that the node did not answer for stays unrepresentable.
    Ok(Some(Reading { head, end, confirmed: effects.list, anchor, tokens: meta, transfers_incomplete, unrepresentable, ignored_unpriced }))
}

/// What the balances changed by that the transfers do not explain: for each asset, the new
/// balance less the old one and the effects between them, when it is more than rounding. Missing
/// internal transfers, rebasing tokens and a layer-2's data fee all end up here.
pub fn unexplained(previous: &BTreeMap<String, Decimal>, confirmed: &[Effect], anchor: &BTreeMap<String, Decimal>, decimals_of: &dyn Fn(&str) -> u32) -> BTreeMap<String, Decimal> {
    let mut explained: BTreeMap<&str, Decimal> = BTreeMap::new();
    for effect in confirmed {
        *explained.entry(effect.asset.as_str()).or_default() += effect.delta;
    }
    let assets: BTreeSet<&str> = previous.keys().chain(anchor.keys()).map(String::as_str).chain(explained.keys().copied()).collect();
    let mut out = BTreeMap::new();
    for asset in assets {
        let before = previous.get(asset).copied().unwrap_or_default();
        let after = anchor.get(asset).copied().unwrap_or_default();
        let difference = after - before - explained.get(asset).copied().unwrap_or_default();
        let dust = Decimal::new(DUST_UNITS as i64, decimals_of(asset));
        if difference.abs() > dust {
            out.insert(asset.to_string(), difference);
        }
    }
    out
}
