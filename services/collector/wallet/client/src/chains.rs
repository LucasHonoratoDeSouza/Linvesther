//! The EVM networks an address can be read on: which explorer serves each, how its native
//! asset is priced, and how many blocks must pass before a block is trusted not to be
//! replaced. An address is the same on every EVM network, so one connection reads all the
//! enabled ones.

use crate::explorer::Endpoint;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chain {
    pub id: u64,
    /// Also the prefix of every asset on it (`base:0x...`, `base:native`).
    pub name: &'static str,
    pub native_symbol: &'static str,
    /// The native asset's price id at DefiLlama.
    pub native_price_id: &'static str,
    /// The prefix DefiLlama gives this network's tokens.
    pub price_prefix: &'static str,
    /// Blocks after which a block is treated as final. Reorganisations of a network deeper
    /// than this are not expected, and it must stay within the recent state a public node keeps
    /// (as few as ~60 blocks on some), because balances are read as of that block.
    pub confirmations: u64,
    /// A public node for reading balances as of a block. It can be replaced per network.
    pub rpc_url: &'static str,
    /// A Blockscout instance, when one serves this network.
    pub blockscout_url: Option<&'static str>,
    /// Whether Etherscan's free plan covers it (the paid plan covers all of them).
    pub etherscan_free: bool,
}

pub const CHAINS: &[Chain] = &[
    Chain { id: 1, name: "ethereum", native_symbol: "ETH", native_price_id: "coingecko:ethereum", price_prefix: "ethereum", confirmations: 32, rpc_url: "https://ethereum-rpc.publicnode.com", blockscout_url: Some("https://eth.blockscout.com"), etherscan_free: true },
    Chain { id: 8453, name: "base", native_symbol: "ETH", native_price_id: "coingecko:ethereum", price_prefix: "base", confirmations: 30, rpc_url: "https://base-rpc.publicnode.com", blockscout_url: Some("https://base.blockscout.com"), etherscan_free: false },
    Chain { id: 42161, name: "arbitrum", native_symbol: "ETH", native_price_id: "coingecko:ethereum", price_prefix: "arbitrum", confirmations: 20, rpc_url: "https://arbitrum-one-rpc.publicnode.com", blockscout_url: Some("https://arbitrum.blockscout.com"), etherscan_free: true },
    Chain { id: 10, name: "optimism", native_symbol: "ETH", native_price_id: "coingecko:ethereum", price_prefix: "optimism", confirmations: 30, rpc_url: "https://optimism-rpc.publicnode.com", blockscout_url: Some("https://explorer.optimism.io"), etherscan_free: false },
    Chain { id: 137, name: "polygon", native_symbol: "POL", native_price_id: "coingecko:polygon-ecosystem-token", price_prefix: "polygon", confirmations: 64, rpc_url: "https://polygon-bor-rpc.publicnode.com", blockscout_url: Some("https://polygon.blockscout.com"), etherscan_free: true },
    Chain { id: 56, name: "bnb", native_symbol: "BNB", native_price_id: "coingecko:binancecoin", price_prefix: "bsc", confirmations: 20, rpc_url: "https://bsc-rpc.publicnode.com", blockscout_url: None, etherscan_free: false },
    Chain { id: 43114, name: "avalanche", native_symbol: "AVAX", native_price_id: "coingecko:avalanche-2", price_prefix: "avax", confirmations: 20, rpc_url: "https://avalanche-c-chain-rpc.publicnode.com", blockscout_url: None, etherscan_free: false },
];

/// The networks read when none are chosen.
pub const DEFAULT_CHAINS: &[u64] = &[1, 8453, 42161, 10, 137];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChainConfigError {
    #[error("{0:?} is not a network id")]
    NotANumber(String),
    #[error("network {0} is not one this can read")]
    Unknown(u64),
    #[error("{name} cannot be read without a key: set ETHERSCAN_API_KEY (its plan must cover the network){}", if *.blockscout { " or BLOCKSCOUT_API_KEY (free at dev.blockscout.com)" } else { "" })]
    NeedsKey { name: &'static str, blockscout: bool },
    #[error("no networks are enabled")]
    Empty,
}

impl Chain {
    pub fn by_id(id: u64) -> Option<&'static Chain> {
        CHAINS.iter().find(|chain| chain.id == id)
    }

    pub fn by_name(name: &str) -> Option<&'static Chain> {
        CHAINS.iter().find(|chain| chain.name == name)
    }

    /// The asset id of this network's native asset.
    pub fn native_asset(&self) -> String {
        format!("{}:native", self.name)
    }

    /// The asset id of a token on this network.
    pub fn token_asset(&self, contract: &str) -> String {
        format!("{}:{}", self.name, contract.to_lowercase())
    }

    /// The name DefiLlama knows a token on this network by.
    pub fn token_price_id(&self, contract: &str) -> String {
        format!("{}:{}", self.price_prefix, contract.to_lowercase())
    }

    /// Which explorer serves this network. Etherscan where its free plan covers the network and a
    /// key is set; else a Blockscout instance when there is one and a key for it; else Etherscan,
    /// if a key is set (a paid plan is then assumed). Without a key an explorer answers only a
    /// handful of calls an hour, so none is used: the network cannot be read until one is set.
    pub fn endpoint(&self, etherscan_key: Option<&str>, blockscout_key: Option<&str>) -> Result<Endpoint, ChainConfigError> {
        let etherscan = |key: &str| Endpoint::EtherscanV2 { api_key: key.to_string(), chain_id: self.id };
        match (self.etherscan_free, etherscan_key, self.blockscout_url, blockscout_key) {
            (true, Some(key), _, _) => Ok(etherscan(key)),
            (_, _, Some(url), Some(key)) => Ok(Endpoint::Blockscout { base_url: url.to_string(), api_key: Some(key.to_string()) }),
            (_, Some(key), _, _) => Ok(etherscan(key)),
            _ => Err(ChainConfigError::NeedsKey { name: self.name, blockscout: self.blockscout_url.is_some() }),
        }
    }
}

/// The price id (DefiLlama's name) of an asset id (`ethereum:native`, `base:0xcontract`), from
/// what the id itself says.
pub fn price_id(asset: &str) -> Option<String> {
    let (network, rest) = asset.split_once(':')?;
    let chain = Chain::by_name(network)?;
    if rest == "native" {
        Some(chain.native_price_id.to_string())
    } else if rest.starts_with("0x") {
        Some(chain.token_price_id(rest))
    } else {
        None
    }
}

/// The networks named in a comma-separated list of ids (`1,8453`), each once, or the default
/// ones when the list is empty.
pub fn parse_enabled(list: &str) -> Result<Vec<&'static Chain>, ChainConfigError> {
    let ids: Vec<u64> = if list.trim().is_empty() {
        DEFAULT_CHAINS.to_vec()
    } else {
        list.split(',').map(|part| part.trim().parse().map_err(|_| ChainConfigError::NotANumber(part.trim().to_string()))).collect::<Result<_, _>>()?
    };
    let mut chains: Vec<&'static Chain> = Vec::new();
    for id in ids {
        let chain = Chain::by_id(id).ok_or(ChainConfigError::Unknown(id))?;
        if !chains.contains(&chain) {
            chains.push(chain);
        }
    }
    if chains.is_empty() {
        return Err(ChainConfigError::Empty);
    }
    Ok(chains)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_ids_are_unique_and_every_network_can_be_priced() {
        for (i, a) in CHAINS.iter().enumerate() {
            for b in &CHAINS[i + 1..] {
                assert!(a.id != b.id && a.name != b.name && a.price_prefix != b.price_prefix);
            }
            assert!(a.confirmations > 0 && a.native_price_id.starts_with("coingecko:"));
            assert!(a.confirmations <= 64, "{} reads balances too far behind the head for a public node", a.name);
        }
    }

    #[test]
    fn assets_are_named_by_network_and_contract_in_lower_case() {
        let base = Chain::by_id(8453).unwrap();
        assert_eq!(base.native_asset(), "base:native");
        assert_eq!(base.token_asset("0xAbC"), "base:0xabc");
        assert_eq!(Chain::by_id(56).unwrap().token_price_id("0xAbC"), "bsc:0xabc");
    }

    #[test]
    fn an_asset_id_says_how_to_price_it() {
        assert_eq!(price_id("ethereum:native").as_deref(), Some("coingecko:ethereum"));
        assert_eq!(price_id("polygon:native").as_deref(), Some("coingecko:polygon-ecosystem-token"));
        assert_eq!(price_id("bnb:0xAbC").as_deref(), Some("bsc:0xabc"));
        assert_eq!(price_id("nowhere:native"), None);
        assert_eq!(price_id("base:nonsense"), None);
    }

    #[test]
    fn a_list_names_each_network_once_and_an_empty_one_means_the_defaults() {
        assert_eq!(parse_enabled("1, 8453,1").unwrap().iter().map(|c| c.id).collect::<Vec<_>>(), vec![1, 8453]);
        assert_eq!(parse_enabled("  ").unwrap().iter().map(|c| c.id).collect::<Vec<_>>(), DEFAULT_CHAINS.to_vec());
        assert_eq!(parse_enabled("1,999999"), Err(ChainConfigError::Unknown(999_999)));
        assert!(matches!(parse_enabled("1,eth"), Err(ChainConfigError::NotANumber(_))));
    }

    #[test]
    fn a_network_is_served_by_the_explorer_that_can_and_none_is_used_without_a_key() {
        let ethereum = Chain::by_id(1).unwrap();
        let base = Chain::by_id(8453).unwrap();
        let bnb = Chain::by_id(56).unwrap();
        // Etherscan takes the networks its free plan covers; Blockscout serves the rest it has an instance for.
        assert!(matches!(ethereum.endpoint(Some("e"), None), Ok(Endpoint::EtherscanV2 { chain_id: 1, .. })));
        assert!(matches!(base.endpoint(Some("e"), Some("b")), Ok(Endpoint::Blockscout { api_key: Some(_), .. })));
        assert!(matches!(ethereum.endpoint(None, Some("b")), Ok(Endpoint::Blockscout { .. })));
        // A network Blockscout does not serve can only be read through a (paid) Etherscan plan.
        assert!(matches!(bnb.endpoint(Some("e"), Some("b")), Ok(Endpoint::EtherscanV2 { chain_id: 56, .. })));
        // A paid-only network with only an Etherscan key is tried through Etherscan.
        assert!(matches!(base.endpoint(Some("e"), None), Ok(Endpoint::EtherscanV2 { chain_id: 8453, .. })));
        // Without any key nothing is read, and the message says which to set.
        let error = base.endpoint(None, None).unwrap_err();
        assert_eq!(error, ChainConfigError::NeedsKey { name: "base", blockscout: true });
        assert!(error.to_string().contains("ETHERSCAN_API_KEY") && error.to_string().contains("BLOCKSCOUT_API_KEY"));
        assert!(!bnb.endpoint(None, Some("b")).unwrap_err().to_string().contains("BLOCKSCOUT"), "Blockscout does not serve bnb, so it is not offered");
    }
}
