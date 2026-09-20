//! Internal transfer linkage, per the protocol specification:
//! "Transferência entre membros só é interna com vínculo inequívoco: IDs
//! da fonte ou identificador de transação/rede + ambos os lados + ativo,
//! quantidade e fees reconciliados. Igualdade de valor e horário não
//! basta... Fee reduz performance. Sem prova do vínculo, a consolidação
//! permanece pendente/incompleta."

use crate::aggregate::ConsolidationError;
use rust_decimal::Decimal;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferLeg {
    pub account_id: String,
    pub asset: String,
    /// Unsigned magnitude.
    pub amount: Decimal,
}

/// A transfer both legs of which are proven to belong to the same
/// underlying movement (same `link_id` — a source transaction/network
/// identifier, never inferred from matching amount and timing alone) and
/// whose amounts reconcile against a fee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalTransfer {
    pub link_id: String,
    pub asset: String,
    pub outgoing: TransferLeg,
    pub incoming: TransferLeg,
    /// `>= 0`; reconciled as `outgoing.amount == incoming.amount + fee`.
    pub fee: Decimal,
}

impl InternalTransfer {
    /// The transfer's contribution to the *consolidated* external-flow
    /// timeline: always zero. This is not a computed value that could
    /// accidentally come out nonzero — the type has no other field this
    /// could read from — because principal moving between two members of
    /// the same set is not external capital movement at the aggregate
    /// level ("transferência interna conta uma vez": once, as an internal
    /// netting, never again as two external legs). The fee is not
    /// modeled as a flow either: it already reduced the incoming leg's
    /// amount, so it shows up naturally as a lower aggregate NAV at the
    /// next valuation — exactly the -1% in the spec's fee vector,
    /// produced with no separate flow bookkeeping.
    pub fn consolidated_external_flow(&self) -> Decimal {
        Decimal::ZERO
    }
}

/// Links `outgoing` and `incoming` into one [`InternalTransfer`] if, and
/// only if: both accounts are current members, they are not the same
/// account, both legs are in the same asset, and the amounts reconcile
/// against `fee`. Any failure means the claimed link is not admissible —
/// per the spec, the two legs then stay unlinked (pending/incomplete),
/// never silently paired because their value and timing merely look
/// similar.
pub fn link_internal_transfer(
    link_id: &str,
    outgoing: TransferLeg,
    incoming: TransferLeg,
    fee: Decimal,
    member_ids: &BTreeSet<String>,
) -> Result<InternalTransfer, ConsolidationError> {
    if !member_ids.contains(&outgoing.account_id) || !member_ids.contains(&incoming.account_id) {
        return Err(ConsolidationError::NotAMember {
            outgoing_account: outgoing.account_id.clone(),
            incoming_account: incoming.account_id.clone(),
        });
    }
    if outgoing.account_id == incoming.account_id {
        return Err(ConsolidationError::SameAccount {
            account_id: outgoing.account_id.clone(),
        });
    }
    if outgoing.asset != incoming.asset {
        return Err(ConsolidationError::AssetMismatch {
            outgoing_asset: outgoing.asset.clone(),
            incoming_asset: incoming.asset.clone(),
        });
    }
    let expected_outgoing = incoming.amount + fee;
    if outgoing.amount != expected_outgoing {
        return Err(ConsolidationError::UnreconciledAmounts {
            outgoing: outgoing.amount,
            incoming_plus_fee: expected_outgoing,
        });
    }

    let asset = outgoing.asset.clone();
    Ok(InternalTransfer {
        link_id: link_id.to_string(),
        asset,
        outgoing,
        incoming,
        fee,
    })
}
