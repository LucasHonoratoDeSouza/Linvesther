//! The structured verification report, per
//! the protocol specification's step 10: "Retornar resultado
//! estruturado por dimensão e reasons; um booleano global sem contexto
//! é insuficiente" — and its "Divulgação e atualidade" section:
//! "Offline: resultado `VALID_AS_OF(snapshot)` ou motivo de falha; não
//! afirmar inexistência de correções posteriores."

use crate::trust::OriginAssessment;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    Pass,
    Fail(String),
    /// The bundle/inputs did not carry what this check needs — never
    /// treated as a pass, but distinct from a real, checked failure.
    Unavailable(String),
}

impl CheckOutcome {
    pub fn is_pass(&self) -> bool {
        matches!(self, CheckOutcome::Pass)
    }
}

/// `Offline` never claims the bundle is *currently* valid — only that it
/// was valid as of `as_of_ms`. There is no `Mode` variant that could be
/// mistaken for "current," so a caller cannot accidentally construct or
/// print a freshness claim this crate didn't actually check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Offline { as_of_ms: i64 },
    Online,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub mode: Mode,
    /// "tamper" failure mode.
    pub hashes: CheckOutcome,
    /// "root" failure mode.
    pub account_set_root: CheckOutcome,
    /// "image" failure mode.
    pub calculation: CheckOutcome,
    /// "chain" failure mode.
    pub registry: CheckOutcome,
    /// "coverage" failure mode.
    pub coverage: CheckOutcome,
    /// Whose data the calculation ran over. Independent of the five
    /// checks above: a proof can be perfectly valid and self-attested.
    pub origin: OriginAssessment,
}

impl VerificationReport {
    pub fn all_pass(&self) -> bool {
        self.hashes.is_pass()
            && self.account_set_root.is_pass()
            && self.calculation.is_pass()
            && self.registry.is_pass()
            && self.coverage.is_pass()
    }

    /// The origin dimension on its own, so a caller can never read a
    /// passing calculation as a claim about the data's honesty.
    pub fn origin_summary(&self) -> String {
        format!("ORIGIN={}", self.origin.label())
    }

    /// A one-line summary that, in offline mode, always reads
    /// `VALID_AS_OF(<timestamp>)` on success — never "VALID" or
    /// "CURRENT" — so a caller cannot mistake it for a freshness claim.
    pub fn summary(&self) -> String {
        if !self.all_pass() {
            return "INVALID".to_string();
        }
        match self.mode {
            Mode::Offline { as_of_ms } => format!("VALID_AS_OF({as_of_ms})"),
            Mode::Online => "VALID".to_string(),
        }
    }
}
