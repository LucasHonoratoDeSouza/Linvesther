//! Reports (denúncias), per the protocol specification: "Denúncia é
//! uma alegação não validada, mantida fora do estado reputacional
//! normativo até haver evidência admissível... nem quantidade de
//! denúncias nem assinatura do owner bastam para invalidar números."

use crate::version::Version;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub target_digest: [u8; 32],
    pub allegation: String,
}

/// Folds a batch of reports over `version`. There is no `Authority` or
/// `evidence_digest` on [`Report`] for this function to read, so it has
/// nothing to promote a report into a [`crate::authority::Correction`]
/// with — reports never change `version`'s status, no matter how many
/// there are.
pub fn apply_reports(version: Version, reports: &[Report]) -> Version {
    let _ = reports.len();
    version
}
