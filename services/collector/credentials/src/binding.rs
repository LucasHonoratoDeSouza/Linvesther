//! Pins an authenticated account's identity context (UID, sub-account,
//! environment) on first use and detects any later mismatch.
//!
//! Per the Binance adapter specification: "Comprometer venueId=binance-global,
//! ambiente, UID autenticado, contexto de subconta e perímetro SPOT... A
//! API key pode mudar mantendo UID e account context; rotação registra
//! CredentialRotated... Troca de UID cria nova conta e novo início."
//!
//! This module does not call Binance itself — it validates an
//! already-parsed [`AccountContext`] the caller obtained from a live
//! request against the context recorded when the credential was first
//! bound, so the same logic is testable without live credentials.

use crate::vault::Environment;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AccountContext {
    pub uid: String,
    pub sub_account_id: Option<String>,
    pub environment: Environment,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BindingError {
    #[error(
        "UID changed from {expected} to {actual}: this is a new account, not a credential rotation"
    )]
    UidMismatch { expected: String, actual: String },
    #[error("sub-account context changed from {expected:?} to {actual:?}")]
    SubAccountMismatch {
        expected: Option<String>,
        actual: Option<String>,
    },
    #[error("environment changed from {expected:?} to {actual:?}: test and production never share a binding")]
    EnvironmentMismatch {
        expected: Environment,
        actual: Environment,
    },
}

/// A pinned binding: the context observed the first time this credential
/// was validated, immutable afterward except by detecting a mismatch
/// (never by silently accepting a new value).
#[derive(Debug, Clone)]
pub struct PinnedBinding {
    expected: AccountContext,
}

impl PinnedBinding {
    pub fn pin(context: AccountContext) -> Self {
        Self { expected: context }
    }

    pub fn expected(&self) -> &AccountContext {
        &self.expected
    }

    /// Checks `observed` against the pinned context. UID and environment
    /// mismatches are always rejected — per the spec, a UID change is a
    /// different account entirely, and environment (test vs. production)
    /// must never silently swap. A missing `sub_account_id` on either
    /// side is treated as "root account", not a wildcard match.
    pub fn check(&self, observed: &AccountContext) -> Result<(), BindingError> {
        if observed.uid != self.expected.uid {
            return Err(BindingError::UidMismatch {
                expected: self.expected.uid.clone(),
                actual: observed.uid.clone(),
            });
        }
        if observed.environment != self.expected.environment {
            return Err(BindingError::EnvironmentMismatch {
                expected: self.expected.environment,
                actual: observed.environment,
            });
        }
        if observed.sub_account_id != self.expected.sub_account_id {
            return Err(BindingError::SubAccountMismatch {
                expected: self.expected.sub_account_id.clone(),
                actual: observed.sub_account_id.clone(),
            });
        }
        Ok(())
    }
}
