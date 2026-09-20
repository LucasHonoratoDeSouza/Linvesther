//! The 91-day purge gate, per
//! the operations design: "O gate de
//! retenção simula pelo menos 91 dias, elimina raw elegível em scratch
//! e exige provar/verificar o próximo checkpoint e as janelas padrão a
//! partir das evidências preservadas... Antes de excluir inputs
//! necessários a um track ativo, é obrigatório ter receipt recursiva
//! real, validada, ancorada e exportável... Se recursão ainda não
//! estiver implementada ou validada, reter integralmente os inputs."

pub const SIMULATION_DAYS: u32 = 91;

/// Whether a validated recursive receipt exists that continues the
/// track's accumulated state. `NotAvailable` covers both "recursion
/// isn't implemented yet" and "one was attempted but didn't validate" —
/// either way, the raw inputs must be retained integrally. This
/// codebase does not implement recursive composition yet (see
/// `zkvm/methods/performance`'s README), so every real call site
/// currently passes `NotAvailable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecursiveReceiptStatus {
    Validated,
    NotAvailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PurgeDecision {
    Purge,
    RetainIntegral { reason: String },
}

/// `days_elapsed` is compared against [`SIMULATION_DAYS`] and, even past
/// that point, purging raw inputs needed by an active track requires
/// `RecursiveReceiptStatus::Validated` — there is no other way to reach
/// `PurgeDecision::Purge` from this function.
pub fn evaluate_purge_gate(
    days_elapsed: u32,
    receipt_status: RecursiveReceiptStatus,
) -> PurgeDecision {
    if days_elapsed < SIMULATION_DAYS {
        return PurgeDecision::RetainIntegral {
            reason: format!(
                "only {days_elapsed} day(s) elapsed; the gate simulates at least {SIMULATION_DAYS}"
            ),
        };
    }
    match receipt_status {
        RecursiveReceiptStatus::Validated => PurgeDecision::Purge,
        RecursiveReceiptStatus::NotAvailable => {
            PurgeDecision::RetainIntegral { reason: "no validated recursive receipt continues the track; raw inputs must be retained integrally".to_string() }
        }
    }
}
