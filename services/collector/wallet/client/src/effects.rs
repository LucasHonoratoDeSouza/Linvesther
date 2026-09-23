//! What each transaction did to an address's balances: the ether and tokens it received or sent,
//! and the gas it paid. Transfers of one transaction are added per asset, so a swap is a debit of
//! one asset and a credit of another in the same transaction.

use crate::amounts::{units, NATIVE_DECIMALS};
use crate::chains::Chain;
use crate::wire::{InternalTx, NormalTx, TokenTransfer};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EffectKind {
    /// Ether or tokens received or sent.
    Transfer,
    /// Gas paid for a transaction the address sent.
    Gas,
}

/// A change to one asset's balance caused by one transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effect {
    pub tx_hash: String,
    pub block: u64,
    pub time_ms: u64,
    /// `ethereum:native` or `base:0xcontract`.
    pub asset: String,
    /// Signed, in the asset's own unit: negative when the balance went down.
    pub delta: Decimal,
    pub kind: EffectKind,
}

#[derive(Debug, Default)]
pub struct Effects {
    pub list: Vec<Effect>,
    /// Assets with a transfer too large to represent exactly. Their balance cannot be followed.
    pub unrepresentable: BTreeSet<String>,
}

/// The effects on `address` of the given transactions and transfers (all on `chain`).
pub fn effects_of(chain: &Chain, address: &str, normal: &[NormalTx], internal: &[InternalTx], tokens: &[TokenTransfer]) -> Effects {
    let me = address.to_lowercase();
    let native = chain.native_asset();
    // (transaction, asset, kind) -> (block, time, sum)
    let mut sums: BTreeMap<(String, String, EffectKind), (u64, u64, Decimal)> = BTreeMap::new();
    let mut unrepresentable = BTreeSet::new();
    let mut add = |hash: &str, block: u64, time_ms: u64, asset: &str, delta: Decimal, kind: EffectKind| {
        let entry = sums.entry((hash.to_string(), asset.to_string(), kind)).or_insert((block, time_ms, Decimal::ZERO));
        entry.2 += delta;
    };

    for tx in normal {
        if tx.from == me {
            // Gas is paid even when the transaction fails.
            let fee_wei = tx.gas_used.parse::<u128>().ok().zip(tx.gas_price.parse::<u128>().ok()).and_then(|(used, price)| used.checked_mul(price));
            let fee = fee_wei.and_then(|wei| units(&wei.to_string(), NATIVE_DECIMALS));
            match fee {
                Some(fee) => add(&tx.hash, tx.block, tx.time_ms, &native, -fee, EffectKind::Gas),
                None => {
                    unrepresentable.insert(native.clone());
                }
            }
        }
        if tx.failed {
            continue;
        }
        match units(&tx.value, NATIVE_DECIMALS) {
            Some(value) => {
                if tx.to == me {
                    add(&tx.hash, tx.block, tx.time_ms, &native, value, EffectKind::Transfer);
                }
                if tx.from == me {
                    add(&tx.hash, tx.block, tx.time_ms, &native, -value, EffectKind::Transfer);
                }
            }
            None => {
                unrepresentable.insert(native.clone());
            }
        }
    }
    for tx in internal.iter().filter(|tx| !tx.failed) {
        match units(&tx.value, NATIVE_DECIMALS) {
            Some(value) => {
                if tx.to == me {
                    add(&tx.hash, tx.block, tx.time_ms, &native, value, EffectKind::Transfer);
                }
                if tx.from == me {
                    add(&tx.hash, tx.block, tx.time_ms, &native, -value, EffectKind::Transfer);
                }
            }
            None => {
                unrepresentable.insert(native.clone());
            }
        }
    }
    for transfer in tokens {
        let asset = chain.token_asset(&transfer.contract);
        match units(&transfer.value, transfer.decimals) {
            Some(value) => {
                if transfer.to == me {
                    add(&transfer.hash, transfer.block, transfer.time_ms, &asset, value, EffectKind::Transfer);
                }
                if transfer.from == me {
                    add(&transfer.hash, transfer.block, transfer.time_ms, &asset, -value, EffectKind::Transfer);
                }
            }
            None => {
                unrepresentable.insert(asset);
            }
        }
    }

    let mut list: Vec<Effect> = sums
        .into_iter()
        .filter(|(_, (_, _, delta))| !delta.is_zero())
        .map(|((tx_hash, asset, kind), (block, time_ms, delta))| Effect { tx_hash, block, time_ms, asset, delta, kind })
        .collect();
    list.sort_by(|a, b| (a.block, &a.tx_hash, &a.asset, a.kind).cmp(&(b.block, &b.tx_hash, &b.asset, b.kind)));
    Effects { list, unrepresentable }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: &str = "0xaaaa";
    const OTHER: &str = "0xbbbb";

    fn chain() -> &'static Chain {
        Chain::by_id(1).unwrap()
    }
    fn normal(hash: &str, block: u64, from: &str, to: &str, value: &str, failed: bool) -> NormalTx {
        NormalTx { hash: hash.into(), block, time_ms: block * 1000, from: from.into(), to: to.into(), value: value.into(), gas_used: "21000".into(), gas_price: "10000000000".into(), failed }
    }
    fn token(hash: &str, block: u64, contract: &str, from: &str, to: &str, value: &str, decimals: u32) -> TokenTransfer {
        TokenTransfer { hash: hash.into(), block, time_ms: block * 1000, contract: contract.into(), symbol: "T".into(), decimals, from: from.into(), to: to.into(), value: value.into(), log_index: None }
    }
    fn find<'a>(effects: &'a Effects, asset: &str, kind: EffectKind) -> Vec<&'a Effect> {
        effects.list.iter().filter(|e| e.asset == asset && e.kind == kind).collect()
    }

    #[test]
    fn ether_received_and_sent_move_the_balance_and_the_sender_pays_gas_in_ether() {
        let txs = [normal("0x1", 10, OTHER, ME, "2000000000000000000", false), normal("0x2", 11, ME, OTHER, "500000000000000000", false)];
        let e = effects_of(chain(), ME, &txs, &[], &[]);
        assert_eq!(find(&e, "ethereum:native", EffectKind::Transfer).iter().map(|x| x.delta.to_string()).collect::<Vec<_>>(), vec!["2.000000000000000000", "-0.500000000000000000"]);
        let gas = find(&e, "ethereum:native", EffectKind::Gas);
        assert_eq!(gas.len(), 1, "only the transaction the address sent costs it gas");
        assert_eq!(gas[0].delta.to_string(), "-0.000210000000000000");
    }

    #[test]
    fn a_failed_transaction_moves_no_value_but_still_costs_gas() {
        let e = effects_of(chain(), ME, &[normal("0x1", 10, ME, OTHER, "1000000000000000000", true)], &[], &[]);
        assert!(find(&e, "ethereum:native", EffectKind::Transfer).is_empty());
        assert_eq!(find(&e, "ethereum:native", EffectKind::Gas).len(), 1);
    }

    #[test]
    fn a_swap_is_one_transaction_with_a_credit_and_a_debit_of_different_assets() {
        // The address pays 100 USDC (6 decimals) and receives 0.04 ether from the router, an internal transfer.
        let tokens = [token("0xs", 20, "0xusdc", ME, OTHER, "100000000", 6)];
        let internal = [InternalTx { hash: "0xs".into(), block: 20, time_ms: 20_000, from: OTHER.into(), to: ME.into(), value: "40000000000000000".into(), index: "1".into(), failed: false }];
        let e = effects_of(chain(), ME, &[], &internal, &tokens);
        assert_eq!(find(&e, "ethereum:0xusdc", EffectKind::Transfer)[0].delta.to_string(), "-100.000000");
        assert_eq!(find(&e, "ethereum:native", EffectKind::Transfer)[0].delta.to_string(), "0.040000000000000000");
        assert!(e.list.iter().all(|x| x.tx_hash == "0xs"));
    }

    #[test]
    fn a_transfer_to_oneself_and_transfers_that_cancel_leave_no_effect() {
        let e = effects_of(chain(), ME, &[normal("0x1", 5, ME, ME, "7", false)], &[], &[token("0x2", 5, "0xt", ME, ME, "5", 0)]);
        assert!(find(&e, "ethereum:native", EffectKind::Transfer).is_empty(), "sent and received in the same transaction");
        assert!(find(&e, "ethereum:0xt", EffectKind::Transfer).is_empty());
    }

    #[test]
    fn transfers_of_others_and_failed_internal_transfers_are_ignored() {
        let internal = [InternalTx { hash: "0x9".into(), block: 1, time_ms: 1000, from: OTHER.into(), to: ME.into(), value: "5".into(), index: "0".into(), failed: true }];
        let e = effects_of(chain(), ME, &[normal("0x1", 1, OTHER, "0xcccc", "9", false)], &internal, &[token("0x3", 1, "0xt", OTHER, "0xcccc", "3", 0)]);
        assert!(e.list.is_empty());
    }

    #[test]
    fn a_token_amount_too_large_to_hold_marks_the_asset_instead_of_being_rounded() {
        let huge = "9".repeat(40);
        let e = effects_of(chain(), ME, &[], &[], &[token("0x1", 1, "0xhuge", OTHER, ME, &huge, 0)]);
        assert!(e.list.is_empty());
        assert!(e.unrepresentable.contains("ethereum:0xhuge"));
    }

    #[test]
    fn the_same_contract_on_another_network_is_another_asset() {
        let base = Chain::by_id(8453).unwrap();
        let e = effects_of(base, ME, &[], &[], &[token("0x1", 1, "0xusdc", OTHER, ME, "1", 0)]);
        assert_eq!(e.list[0].asset, "base:0xusdc");
    }
}
