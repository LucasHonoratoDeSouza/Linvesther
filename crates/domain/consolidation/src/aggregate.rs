//! Aggregate (consolidated) NAV across a track's member accounts, per
//! the protocol specification: "Em qualquer corte, incluir todas as
//! contas membros... Mudança em qualquer conta afeta o cálculo agregado,
//! nunca só um detalhe oculto de UI."

use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberAccount {
    pub account_id: String,
    /// Already valued in the consolidated currency at this cut (e.g. via
    /// `crates/domain/valuation`).
    pub nav: Decimal,
    /// Whether this member has a coverage gap at this cut.
    pub has_gap: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConsolidationError {
    #[error("member {account_id} has a coverage gap at this cut: the aggregate is unavailable, not partially computed")]
    MemberGap { account_id: String },
    #[error(
        "member {outgoing_account}/{incoming_account} are not both current members of the set"
    )]
    NotAMember {
        outgoing_account: String,
        incoming_account: String,
    },
    #[error(
        "an internal transfer cannot be between the same account ({account_id}) on both sides"
    )]
    SameAccount { account_id: String },
    #[error("linked transfer legs are in different assets ({outgoing_asset} vs {incoming_asset})")]
    AssetMismatch {
        outgoing_asset: String,
        incoming_asset: String,
    },
    #[error("transfer amounts do not reconcile: outgoing {outgoing} != incoming+fee {incoming_plus_fee}")]
    UnreconciledAmounts {
        outgoing: Decimal,
        incoming_plus_fee: Decimal,
    },
}

/// Sums every member's NAV. Refuses the *entire* aggregate (not a partial
/// sum silently omitting the gapped member) if any single member has a
/// coverage gap at this cut — "gap de membro impede agregado."
pub fn consolidated_nav(members: &[MemberAccount]) -> Result<Decimal, ConsolidationError> {
    for member in members {
        if member.has_gap {
            return Err(ConsolidationError::MemberGap {
                account_id: member.account_id.clone(),
            });
        }
    }
    Ok(members.iter().map(|m| m.nav).sum())
}

/// The external flow an account's *addition* to the set contributes at
/// the aggregate level: exactly its authenticated value at entry, per
/// "A adição funciona como fluxo externo no valor autenticado da conta na
/// entrada... Nenhuma dessas duas alterações cria lucro" — there is no
/// markup, estimate, or rounding beyond the value itself, so the
/// aggregate TWR computed with this flow shows zero return from the
/// addition alone.
pub fn account_added_flow(entry_nav: Decimal) -> Decimal {
    entry_nav
}

/// The external flow an account's *removal* contributes: negative,
/// exactly its last authenticated value. Per the spec, this requires a
/// trustworthy last valuation; if none exists the segment is incomplete
/// (a concern for whoever assembles the checkpoint/gap composition, not
/// this function — it only ever computes the flow from a value the
/// caller already established as valid).
pub fn account_removed_flow(exit_nav: Decimal) -> Decimal {
    -exit_nav
}
