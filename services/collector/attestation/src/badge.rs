//! Origin badge, per the protocol specification: "A0 é assinatura de
//! coletor identificado que observou a API. Não é assinatura da Binance
//! nem resistência a coletor malicioso" and
//! the protocol specification: "dados A0 NÃO DEVEM aparecer
//! como origem TLS."

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginBadge {
    A0,
    A1,
    A2,
}

/// The badge is exactly the envelope's own declared `mechanism` — this
/// function takes no other input, so nothing else (a valid calculation
/// proof, a clean reputation, matching signatures elsewhere) can elevate
/// it. Proving the math correctly never changes what was proven about
/// the origin: an A0 envelope always yields an A0 badge.
pub fn origin_badge(mechanism: &str) -> OriginBadge {
    match mechanism {
        "A1" => OriginBadge::A1,
        "A2" => OriginBadge::A2,
        // "A0" and any unrecognized mechanism are treated identically:
        // an unknown mechanism must never be assumed stronger than A0.
        _ => OriginBadge::A0,
    }
}
