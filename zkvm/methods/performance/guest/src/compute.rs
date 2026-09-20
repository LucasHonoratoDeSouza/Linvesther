//! Reconciliation, TWR index and MDD, per the protocol specification:
//! "Guest inicial prova reconciliação spot, TWR amostrado, MDD diário,
//! duração elegível e predicates de capital/retorno/MDD." This guest delivers a
//! minimal, integer-only version of that scope: no external flows, spot
//! reconciliation only, one NAV sample per checkpoint day. NAV values are
//! fixed-point integers scaled by [`SCALE`] (never `f64`, matching the
//! rest of this codebase's domain crates).

pub const SCALE: i64 = 1_000_000;

#[derive(Debug, PartialEq, Eq)]
pub enum ComputeError {
    EmptyNav,
    NonPositiveNav { index: usize, nav: i64 },
    ReconciliationMismatch { expected: i64, got: i64 },
}

/// Confirms `ledger_deltas` sum to exactly the NAV's net change over the
/// period. Spot reconciliation only: no internal/external flow modeling,
/// so this is a strict equality, not a tolerance band.
pub fn reconcile(nav: &[i64], ledger_deltas: &[i64]) -> Result<(), ComputeError> {
    let first = *nav.first().ok_or(ComputeError::EmptyNav)?;
    let last = *nav.last().ok_or(ComputeError::EmptyNav)?;
    let expected = last - first;
    let got: i64 = ledger_deltas.iter().sum();
    if expected != got {
        return Err(ComputeError::ReconciliationMismatch { expected, got });
    }
    Ok(())
}

/// Chained time-weighted return index, starting at `SCALE` (representing
/// 1.0), over consecutive NAV samples with no flows between them.
pub fn twr_index(nav: &[i64]) -> Result<i128, ComputeError> {
    if nav.is_empty() {
        return Err(ComputeError::EmptyNav);
    }
    let scale = SCALE as i128;
    let mut index: i128 = scale;
    for (i, window) in nav.windows(2).enumerate() {
        let (prev, cur) = (window[0], window[1]);
        if prev <= 0 {
            return Err(ComputeError::NonPositiveNav {
                index: i,
                nav: prev,
            });
        }
        let ratio = (cur as i128 * scale) / prev as i128;
        index = (index * ratio) / scale;
    }
    Ok(index)
}

/// Maximum drawdown, in basis points of magnitude (always `>= 0`), with
/// the earliest-peak/earliest-trough tie-break falling out of a single
/// forward pass using strict inequalities only.
pub fn max_drawdown_bp(nav: &[i64]) -> Result<i64, ComputeError> {
    let first = *nav.first().ok_or(ComputeError::EmptyNav)?;
    if first <= 0 {
        return Err(ComputeError::NonPositiveNav {
            index: 0,
            nav: first,
        });
    }
    let scale = SCALE as i128;
    let mut peak = first as i128;
    let mut max_dd_scaled: i128 = 0;
    for (i, &value) in nav.iter().enumerate() {
        if value <= 0 {
            return Err(ComputeError::NonPositiveNav {
                index: i,
                nav: value,
            });
        }
        let value = value as i128;
        if value > peak {
            peak = value;
        } else {
            let drawdown = ((peak - value) * scale) / peak;
            if drawdown > max_dd_scaled {
                max_dd_scaled = drawdown;
            }
        }
    }
    Ok(((max_dd_scaled * 10_000) / scale) as i64)
}
