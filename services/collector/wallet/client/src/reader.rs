//! What reading an address's chain needs, as a trait: [`LiveReader`] is the real reader — an
//! explorer for the address's transfers and a node for its balances as of a block — and a
//! synthetic chain can stand in for it in a test.

use crate::explorer::{Explorer, ExplorerError};
use crate::rpc::{Rpc, RpcError};
use crate::wire::{HeldToken, InternalTx, Listing, NormalTx, TokenTransfer};

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error(transparent)]
    Explorer(#[from] ExplorerError),
    #[error(transparent)]
    Node(#[from] RpcError),
}

pub trait ChainReader {
    /// The newest block the node knows.
    fn head_block(&self) -> Result<u64, ReadError>;
    /// The newest block the explorer has indexed; transfers are only complete up to it.
    fn indexed_block(&self) -> Result<u64, ReadError>;
    /// The native balance in wei as of `block`.
    fn native_balance_at(&self, address: &str, block: u64) -> Result<String, ReadError>;
    /// Token balances as of `block`, in the order of `contracts`; `None` for a contract that
    /// does not answer.
    fn token_balances_at(&self, address: &str, contracts: &[String], block: u64) -> Result<Vec<Option<String>>, ReadError>;
    /// Tokens the address may hold. Their balances are not to be trusted; read them with
    /// [`ChainReader::token_balances_at`].
    fn tokens(&self, address: &str) -> Result<Vec<HeldToken>, ReadError>;
    fn normal_txs(&self, address: &str, from: u64, to: u64) -> Result<Listing<NormalTx>, ReadError>;
    fn internal_txs(&self, address: &str, from: u64, to: u64) -> Result<Listing<InternalTx>, ReadError>;
    fn token_transfers(&self, address: &str, from: u64, to: u64) -> Result<Listing<TokenTransfer>, ReadError>;
}

/// An explorer and a node for one network.
pub struct LiveReader {
    pub explorer: Explorer,
    pub node: Rpc,
}

impl ChainReader for LiveReader {
    fn head_block(&self) -> Result<u64, ReadError> {
        Ok(self.node.head_block()?)
    }
    fn indexed_block(&self) -> Result<u64, ReadError> {
        Ok(self.explorer.head_block()?)
    }
    fn native_balance_at(&self, address: &str, block: u64) -> Result<String, ReadError> {
        Ok(self.node.native_balance_at(address, block)?)
    }
    fn token_balances_at(&self, address: &str, contracts: &[String], block: u64) -> Result<Vec<Option<String>>, ReadError> {
        Ok(self.node.token_balances_at(address, contracts, block)?)
    }
    fn tokens(&self, address: &str) -> Result<Vec<HeldToken>, ReadError> {
        Ok(self.explorer.tokens(address)?)
    }
    fn normal_txs(&self, address: &str, from: u64, to: u64) -> Result<Listing<NormalTx>, ReadError> {
        Ok(self.explorer.normal_txs(address, from, to)?)
    }
    fn internal_txs(&self, address: &str, from: u64, to: u64) -> Result<Listing<InternalTx>, ReadError> {
        Ok(self.explorer.internal_txs(address, from, to)?)
    }
    fn token_transfers(&self, address: &str, from: u64, to: u64) -> Result<Listing<TokenTransfer>, ReadError> {
        Ok(self.explorer.token_transfers(address, from, to)?)
    }
}
