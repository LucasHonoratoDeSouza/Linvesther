//! Idempotent effect application, per the operations design: "Efeito remoto é
//! reconciliado antes do retry: consultar hash/nonce/tx em vez de
//! transmitir novo payload" and the architecture design: "Transação local grava evento,
//! projeção e outbox juntos... Processamento é at-least-once com efeitos
//! idempotentes."

use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effect {
    pub idempotency_key: String,
    pub payload_digest: [u8; 32],
}

/// Tracks which effects have already been applied by idempotency key, so
/// that a crash between any two effects — followed by re-running the
/// whole job from its start — reapplies only the effects that never
/// completed, never duplicating one that did.
#[derive(Debug, Default)]
pub struct Outbox {
    applied_keys: BTreeSet<String>,
    applied_effects: Vec<Effect>,
}

impl Outbox {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies `effect` unless its idempotency key was already applied.
    /// Returns whether this call actually recorded it (`true` the first
    /// time, `false` on every replay after a crash-and-resume).
    pub fn apply(&mut self, effect: Effect) -> bool {
        if self.applied_keys.insert(effect.idempotency_key.clone()) {
            self.applied_effects.push(effect);
            true
        } else {
            false
        }
    }

    pub fn applied_effects(&self) -> &[Effect] {
        &self.applied_effects
    }
}
