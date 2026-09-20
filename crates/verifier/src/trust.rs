//! Who signed the data a proof was calculated from.
//!
//! A valid receipt shows that a calculation follows from its inputs. It
//! does not show the inputs were honest: anyone can generate a key, sign
//! invented figures and prove the arithmetic over them. The performance
//! guest therefore commits the signing collector's fingerprint into the
//! journal, and this module compares it against a list of collectors the
//! *verifier* trusts. Anything else is reported as self-attested, never
//! silently accepted.

use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TrustListError {
    #[error("the trust list is not valid JSON: {0}")]
    Json(String),
    #[error("unsupported trust list version {0} (expected 1)")]
    UnsupportedVersion(u32),
    #[error("collector \"{name}\": fingerprint must be 64 hex characters")]
    BadFingerprint { name: String },
    #[error("collector \"{name}\": valid-from is after valid-until")]
    EmptyValidity { name: String },
    #[error("fingerprint listed more than once: {0}")]
    Duplicate(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedCollector {
    pub name: String,
    pub fingerprint: [u8; 32],
    pub valid_from_ms: Option<i64>,
    pub valid_until_ms: Option<i64>,
    /// A revoked collector is never trusted, whatever the date: from a
    /// proof alone there is no way to show a signature predates a key
    /// compromise.
    pub revoked: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustList {
    collectors: Vec<TrustedCollector>,
}

#[derive(Deserialize)]
struct WireList {
    version: u32,
    collectors: Vec<WireCollector>,
}

#[derive(Deserialize)]
struct WireCollector {
    name: String,
    fingerprint: String,
    #[serde(rename = "validFromMs")]
    valid_from_ms: Option<i64>,
    #[serde(rename = "validUntilMs")]
    valid_until_ms: Option<i64>,
    #[serde(default)]
    revoked: bool,
}

impl TrustList {
    /// No collector is trusted: every proof reads as self-attested.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_json(json: &str) -> Result<Self, TrustListError> {
        let wire: WireList = serde_json::from_str(json).map_err(|e| TrustListError::Json(e.to_string()))?;
        if wire.version != 1 {
            return Err(TrustListError::UnsupportedVersion(wire.version));
        }
        let mut seen = HashSet::new();
        let mut collectors = Vec::with_capacity(wire.collectors.len());
        for entry in wire.collectors {
            let bytes = hex::decode(&entry.fingerprint).map_err(|_| TrustListError::BadFingerprint { name: entry.name.clone() })?;
            let fingerprint: [u8; 32] = bytes.try_into().map_err(|_| TrustListError::BadFingerprint { name: entry.name.clone() })?;
            if let (Some(from), Some(until)) = (entry.valid_from_ms, entry.valid_until_ms) {
                if from > until {
                    return Err(TrustListError::EmptyValidity { name: entry.name });
                }
            }
            if !seen.insert(fingerprint) {
                return Err(TrustListError::Duplicate(entry.fingerprint));
            }
            collectors.push(TrustedCollector {
                name: entry.name,
                fingerprint,
                valid_from_ms: entry.valid_from_ms,
                valid_until_ms: entry.valid_until_ms,
                revoked: entry.revoked,
            });
        }
        Ok(Self { collectors })
    }

    pub fn lookup(&self, fingerprint: &[u8; 32]) -> Option<&TrustedCollector> {
        self.collectors.iter().find(|c| &c.fingerprint == fingerprint)
    }
}

/// How much the origin of a proof's data can be relied on. Independent of
/// whether the calculation itself verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginAssessment {
    /// Signed by a collector on the verifier's list, valid for the period.
    TrustedCollector { name: String, fingerprint: [u8; 32] },
    /// Signed by a key the verifier does not list. The figures are exactly
    /// as trustworthy as whoever holds that key.
    SelfAttested { fingerprint: [u8; 32] },
    /// The signing collector is on the list but has been revoked.
    Revoked { name: String, fingerprint: [u8; 32] },
    /// The signing collector is listed, but the proof covers a period
    /// outside the dates it is trusted for.
    OutsideValidity { name: String, fingerprint: [u8; 32] },
    /// There was no verified receipt to read a signer from.
    Unavailable(String),
}

impl OriginAssessment {
    pub fn is_trusted(&self) -> bool {
        matches!(self, OriginAssessment::TrustedCollector { .. })
    }

    /// A short label that never reads as a general "valid" claim.
    pub fn label(&self) -> String {
        match self {
            OriginAssessment::TrustedCollector { name, .. } => format!("TRUSTED_COLLECTOR({name})"),
            OriginAssessment::SelfAttested { .. } => "SELF_ATTESTED".to_string(),
            OriginAssessment::Revoked { name, .. } => format!("REVOKED_COLLECTOR({name})"),
            OriginAssessment::OutsideValidity { name, .. } => format!("COLLECTOR_OUTSIDE_VALIDITY({name})"),
            OriginAssessment::Unavailable(_) => "ORIGIN_UNAVAILABLE".to_string(),
        }
    }
}

/// Classifies `fingerprint` against `list`. `period_end_ms` is the end of
/// the period the proof covers, checked against the collector's validity
/// window.
pub fn assess_origin(fingerprint: [u8; 32], period_end_ms: i64, list: &TrustList) -> OriginAssessment {
    let Some(collector) = list.lookup(&fingerprint) else {
        return OriginAssessment::SelfAttested { fingerprint };
    };
    if collector.revoked {
        return OriginAssessment::Revoked { name: collector.name.clone(), fingerprint };
    }
    let too_early = collector.valid_from_ms.is_some_and(|from| period_end_ms < from);
    let too_late = collector.valid_until_ms.is_some_and(|until| period_end_ms > until);
    if too_early || too_late {
        return OriginAssessment::OutsideValidity { name: collector.name.clone(), fingerprint };
    }
    OriginAssessment::TrustedCollector { name: collector.name.clone(), fingerprint }
}
