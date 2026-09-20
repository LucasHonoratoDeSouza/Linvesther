//! Conflicting corrections, per the protocol specification: "Duas
//! versões autenticadas incompatíveis, sem uma relação de supersessão
//! demonstrada pela fonte/policy, resultam em `DISPUTED`: preservar
//! ambas e suspender métricas dependentes. Não escolher maior retorno,
//! autor preferido ou simplesmente última resposta HTTP."

/// Evidence, from the source or its policy, that one version supersedes
/// the other. This is the only way `reconcile_conflicting` can resolve a
/// conflict — there is no parameter carrying a return value, an author
/// preference, or an arrival order, so none of those can factor in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Supersession {
    pub superseding: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Resolved { winner: [u8; 32] },
    Disputed { a: [u8; 32], b: [u8; 32] },
}

pub fn reconcile_conflicting(
    a_digest: [u8; 32],
    b_digest: [u8; 32],
    supersession: Option<Supersession>,
) -> Resolution {
    match supersession {
        Some(s) if s.superseding == a_digest || s.superseding == b_digest => Resolution::Resolved {
            winner: s.superseding,
        },
        _ => Resolution::Disputed {
            a: a_digest,
            b: b_digest,
        },
    }
}
