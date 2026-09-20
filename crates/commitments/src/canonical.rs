//! Canonical JSON encoding for suite `LZK-JCS-SHA256-v1`.
//!
//! RFC 8785 (JCS) canonicalization is delegated to `serde_jcs`, a maintained
//! implementation; this module adds the stricter input rules the protocol
//! requires beyond bare RFC 8785, which `serde_json::Value` alone cannot
//! express because duplicate object keys are already resolved (last write
//! wins) and floats already parsed by the time a `Value` exists: **duplicate
//! object keys and any float-syntax number are rejected outright**, at
//! parse time, rather than silently accepted or resolved.
//!
//! Schema-level string-encoding rules for specific fields (financial
//! amounts and timestamps as sign-free, non-zero-padded integer strings)
//! are a domain/schema concern for the crate that defines those schemas,
//! not this generic codec layer, and are not enforced here.

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum CanonicalError {
    #[error("invalid JSON: {0}")]
    Parse(String),
    #[error("duplicate object key: {0}")]
    DuplicateKey(String),
    #[error(
        "float-syntax number rejected (financial/timestamp fields must be integer strings): {0}"
    )]
    FloatRejected(String),
    #[error("JCS serialization failed: {0}")]
    Jcs(String),
}

/// Parses `json_text`, rejecting duplicate keys and float-syntax numbers,
/// then returns its RFC 8785 canonical UTF-8 byte encoding.
pub fn canonicalize(json_text: &str) -> Result<Vec<u8>, CanonicalError> {
    let mut deserializer = serde_json::Deserializer::from_str(json_text);
    StrictSeed
        .deserialize(&mut deserializer)
        .map_err(|e| CanonicalError::Parse(e.to_string()))?;
    deserializer
        .end()
        .map_err(|e| CanonicalError::Parse(e.to_string()))?;

    // Re-parsing into a Value is redundant work but keeps the strict
    // validator and the canonical serializer decoupled and independently
    // testable; canonicalization only ever runs on already-small protocol
    // objects (headers, events, records), not raw exchange payloads.
    let value: Value =
        serde_json::from_str(json_text).map_err(|e| CanonicalError::Parse(e.to_string()))?;
    canonicalize_value(&value)
}

/// Canonicalizes an already-parsed, already-trusted [`Value`] (e.g. one
/// this crate built itself for a commitment). Does not re-run strict
/// validation, since a `Value` can no longer contain duplicate keys or
/// float-syntax literals as distinct from integers.
pub fn canonicalize_value(value: &Value) -> Result<Vec<u8>, CanonicalError> {
    serde_jcs::to_string(value)
        .map(String::into_bytes)
        .map_err(|e| CanonicalError::Jcs(e.to_string()))
}

/// A `DeserializeSeed` that walks the document purely for validation,
/// discarding the values it visits. Recurses into arrays/objects with the
/// same strict rules so a violation nested at any depth is caught.
struct StrictSeed;

impl<'de> DeserializeSeed<'de> for StrictSeed {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictVisitor)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value with no duplicate keys and no float-syntax numbers")
    }

    fn visit_bool<E>(self, _v: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E>(self, _v: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E>(self, _v: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E>(self, v: f64) -> Result<(), E>
    where
        E: de::Error,
    {
        Err(E::custom(format!("float-syntax number: {v}")))
    }
    fn visit_str<E>(self, _v: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_string<E>(self, _v: String) -> Result<(), E> {
        Ok(())
    }
    fn visit_none<E>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_some<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictVisitor)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        while seq.next_element_seed(StrictSeed)?.is_some() {}
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> Result<(), A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(de::Error::custom(format!("duplicate object key: {key}")));
            }
            map.next_value_seed(StrictSeed)?;
        }
        Ok(())
    }
}
