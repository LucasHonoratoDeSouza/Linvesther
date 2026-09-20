//! Reconstructs one asset's quantity at a common cut time from a
//! non-atomic balance snapshot plus the ledger deltas around it.
//!
//! Per the protocol specification: "Os endpoints/saldos podem não ser
//! atômicos. Reconstruir holdings num corte comum usando eventos
//! autenticados antes/depois do snapshot. Sem ordenação ou cobertura
//! suficientes para estabelecer esse corte, invalidar valuation."

use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BalanceSnapshot {
    pub free: Decimal,
    pub locked: Decimal,
    /// When this specific per-asset balance query returned — may differ
    /// slightly from other assets' snapshot times, since the underlying
    /// endpoint is not atomic across assets.
    pub observed_at_ms: u64,
}

impl BalanceSnapshot {
    /// Free + locked, combined exactly once — per metrics.md: "Incluir
    /// livre + bloqueado em ordens, sem contar bloqueado duas vezes."
    pub fn total(&self) -> Decimal {
        self.free + self.locked
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedDelta {
    pub time_ms: u64,
    pub delta: Decimal,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReconstructError {
    #[error("event at {event_time_ms} exactly coincides with the snapshot instant {snapshot_time_ms}: cannot tell if the snapshot already reflects it")]
    AmbiguousOrdering {
        event_time_ms: u64,
        snapshot_time_ms: u64,
    },
}

/// Reconstructs the asset's total quantity at `cut_ms`, given a snapshot
/// and the ledger deltas around it (sorted or not; every delta is
/// inspected). An event whose `time_ms` exactly equals the snapshot's
/// `observed_at_ms` makes the ordering ambiguous and refuses the whole
/// reconstruction — never resolved by an arbitrary tie-break.
pub fn reconstruct_quantity(
    snapshot: &BalanceSnapshot,
    events: &[TimedDelta],
    cut_ms: u64,
) -> Result<Decimal, ReconstructError> {
    for event in events {
        if event.time_ms == snapshot.observed_at_ms {
            return Err(ReconstructError::AmbiguousOrdering {
                event_time_ms: event.time_ms,
                snapshot_time_ms: snapshot.observed_at_ms,
            });
        }
    }

    let mut quantity = snapshot.total();
    if cut_ms >= snapshot.observed_at_ms {
        // Cut is at or after the snapshot: apply events strictly after
        // the snapshot instant, up to and including the cut.
        for event in events {
            if event.time_ms > snapshot.observed_at_ms && event.time_ms <= cut_ms {
                quantity += event.delta;
            }
        }
    } else {
        // Cut is before the snapshot: undo events strictly after the
        // cut, up to and including the snapshot instant.
        for event in events {
            if event.time_ms > cut_ms && event.time_ms <= snapshot.observed_at_ms {
                quantity -= event.delta;
            }
        }
    }
    Ok(quantity)
}
