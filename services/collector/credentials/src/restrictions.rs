//! Validates `GET /sapi/v1/account/apiRestrictions` per
//! the Binance adapter specification: "Leitura ativa, escrita/saque/trading/
//! transferências desativados... Campos de permissão novos/desconhecidos
//! são fail-closed até classificação; não solicitar poder de mover
//! dinheiro para conseguir consultar histórico."
//!
//! `canTrade`/`canWithdraw` on the account object express account
//! capabilities, not the key's — this module only ever looks at
//! `apiRestrictions` fields, never the account object, matching the
//! spec's explicit distinction.
//!
//! `enable_fix_api_trade`/`enable_fix_read_only`/
//! `enable_portfolio_margin_trading`/`enable_vanilla_options`/
//! `create_time` were not in this module's original field list — they
//! were discovered from a real `apiRestrictions` response during the live homologation
//! live Binance homologation (this repo's first genuinely credentialed
//! run against the API), where they correctly fell into
//! `unknown_fields` and fail-closed as designed. `create_time` is purely
//! informational (a timestamp, not a capability); the three `enable*`
//! fields are additional trade/margin/options capabilities and are now
//! checked the same way every other write capability already is.

use serde::Deserialize;
use std::collections::BTreeMap;

/// Deserializes the known `apiRestrictions` fields explicitly, and
/// collects anything else into `unknown_fields` via `#[serde(flatten)]` —
/// so a new permission Binance adds tomorrow is never silently ignored;
/// it fails closed in [`validate_read_only`] instead.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiRestrictions {
    #[serde(default)]
    pub ip_restrict: bool,
    #[serde(default)]
    pub enable_reading: bool,
    #[serde(default)]
    pub enable_withdrawals: bool,
    #[serde(default)]
    pub enable_internal_transfer: bool,
    #[serde(default)]
    pub enable_universal_transfer: bool,
    #[serde(default)]
    pub enable_spot_and_margin_trading: bool,
    #[serde(default)]
    pub enable_margin: bool,
    #[serde(default)]
    pub enable_futures: bool,
    #[serde(default)]
    pub permits_universal_transfer: bool,
    /// FIX API order-entry access — a trade capability, checked like
    /// any other.
    #[serde(default)]
    pub enable_fix_api_trade: bool,
    /// FIX API market-data access only — read, not a write capability;
    /// intentionally not checked in [`validate_read_only`].
    #[serde(default)]
    pub enable_fix_read_only: bool,
    #[serde(default)]
    pub enable_portfolio_margin_trading: bool,
    #[serde(default)]
    pub enable_vanilla_options: bool,
    /// Informational only (key creation time) — never a capability.
    #[serde(default)]
    pub create_time: Option<u64>,
    #[serde(flatten)]
    pub unknown_fields: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RestrictionsError {
    #[error("apiRestrictions.enableReading is false: key cannot read account data")]
    ReadingDisabled,
    #[error("apiRestrictions grants a write/trade/transfer capability: {0}")]
    WriteCapabilityEnabled(&'static str),
    #[error("apiRestrictions has unclassified fields, fail-closed: {0:?}")]
    UnknownFields(Vec<String>),
}

/// Requires `enableReading == true`, every known write/trade/transfer
/// capability `== false`, and no unrecognized fields present.
pub fn validate_read_only(restrictions: &ApiRestrictions) -> Result<(), RestrictionsError> {
    if !restrictions.unknown_fields.is_empty() {
        let mut names: Vec<String> = restrictions.unknown_fields.keys().cloned().collect();
        names.sort();
        return Err(RestrictionsError::UnknownFields(names));
    }
    if !restrictions.enable_reading {
        return Err(RestrictionsError::ReadingDisabled);
    }
    if restrictions.enable_withdrawals {
        return Err(RestrictionsError::WriteCapabilityEnabled(
            "enableWithdrawals",
        ));
    }
    if restrictions.enable_internal_transfer {
        return Err(RestrictionsError::WriteCapabilityEnabled(
            "enableInternalTransfer",
        ));
    }
    if restrictions.enable_universal_transfer {
        return Err(RestrictionsError::WriteCapabilityEnabled(
            "enableUniversalTransfer",
        ));
    }
    if restrictions.enable_spot_and_margin_trading {
        return Err(RestrictionsError::WriteCapabilityEnabled(
            "enableSpotAndMarginTrading",
        ));
    }
    if restrictions.enable_margin {
        return Err(RestrictionsError::WriteCapabilityEnabled("enableMargin"));
    }
    if restrictions.enable_futures {
        return Err(RestrictionsError::WriteCapabilityEnabled("enableFutures"));
    }
    if restrictions.permits_universal_transfer {
        return Err(RestrictionsError::WriteCapabilityEnabled(
            "permitsUniversalTransfer",
        ));
    }
    if restrictions.enable_fix_api_trade {
        return Err(RestrictionsError::WriteCapabilityEnabled(
            "enableFixApiTrade",
        ));
    }
    if restrictions.enable_portfolio_margin_trading {
        return Err(RestrictionsError::WriteCapabilityEnabled(
            "enablePortfolioMarginTrading",
        ));
    }
    if restrictions.enable_vanilla_options {
        return Err(RestrictionsError::WriteCapabilityEnabled(
            "enableVanillaOptions",
        ));
    }
    Ok(())
}
