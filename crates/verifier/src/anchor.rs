//! Chain/anchor check ("chain" failure mode), per
//! the protocol specification's step 4: "Validar inclusão/finalidade
//! contra binding da rede e um checkpoint de consenso confiável."
//!
//! This crate never calls an RPC itself — "CLI verifica sem API
//! oficial" (the acceptance criteria) means verification runs entirely off
//! locally supplied data. A caller (the CLI, or a future online mode)
//! supplies the trusted, already-finalized anchors independently
//! obtained from a node; this module only checks the bundle's claimed
//! anchor against that trusted set.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrustedAnchor {
    pub tx_hash: String,
    pub block_number: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AnchorError {
    #[error("anchor tx {tx_hash} at block {block_number} is not among the trusted, independently-obtained anchors")]
    NotTrusted { tx_hash: String, block_number: u64 },
}

pub fn verify_anchor(
    claimed: &TrustedAnchor,
    trusted: &HashSet<TrustedAnchor>,
) -> Result<(), AnchorError> {
    if !trusted.contains(claimed) {
        return Err(AnchorError::NotTrusted {
            tx_hash: claimed.tx_hash.clone(),
            block_number: claimed.block_number,
        });
    }
    Ok(())
}
