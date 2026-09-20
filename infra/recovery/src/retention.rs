//! Retention deadlines, per
//! the operations design: "Remoção revoga
//! acesso e elimina credencial em até 24 h; exclusão privada sob pedido
//! leva até 30 dias nos sistemas ativos e até 35 dias em backups." and
//! "isolamento e retenção" (the acceptance criteria).

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataCategory {
    /// Collector credential (API key/secret) — the tightest deadline.
    Credential,
    /// Private data in active systems.
    PrivateActive,
    /// Private data in backups (naturally lags active systems).
    Backup,
}

impl DataCategory {
    pub fn deadline_ms(self) -> i64 {
        match self {
            DataCategory::Credential => 24 * HOUR_MS,
            DataCategory::PrivateActive => 30 * DAY_MS,
            DataCategory::Backup => 35 * DAY_MS,
        }
    }
}

/// The deadline by which `category` must be purged after a removal
/// request made at `requested_at_ms`.
pub fn purge_deadline_ms(category: DataCategory, requested_at_ms: i64) -> i64 {
    requested_at_ms + category.deadline_ms()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionViolation {
    pub category: DataCategory,
    pub deadline_ms: i64,
    pub now_ms: i64,
}

/// `Err` when `category`'s data is still present past its deadline —
/// isolation of one category's timeline from another (a credential
/// overdue does not imply a backup is overdue, and vice versa).
pub fn check_purged_by_deadline(
    category: DataCategory,
    requested_at_ms: i64,
    still_present: bool,
    now_ms: i64,
) -> Result<(), RetentionViolation> {
    let deadline_ms = purge_deadline_ms(category, requested_at_ms);
    if still_present && now_ms > deadline_ms {
        return Err(RetentionViolation {
            category,
            deadline_ms,
            now_ms,
        });
    }
    Ok(())
}
