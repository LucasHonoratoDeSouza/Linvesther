use verifier::{assess_origin, OriginAssessment, TrustList, TrustListError};

const KEY_7: &str = "9c177b47de4c524a7b7754c648e488d3993b12b88930e0d67f9e3bcd01afc134";

fn fingerprint(hex_str: &str) -> [u8; 32] {
    hex::decode(hex_str).unwrap().try_into().unwrap()
}

fn list(entry: &str) -> TrustList {
    TrustList::from_json(&format!(r#"{{"version":1,"collectors":[{entry}]}}"#)).unwrap()
}

#[test]
fn an_unlisted_signer_is_self_attested() {
    let assessment = assess_origin(fingerprint(KEY_7), 1_000, &TrustList::empty());
    assert_eq!(assessment, OriginAssessment::SelfAttested { fingerprint: fingerprint(KEY_7) });
    assert!(!assessment.is_trusted());
}

#[test]
fn a_listed_signer_is_trusted() {
    let trust = list(&format!(r#"{{"name":"example","fingerprint":"{KEY_7}"}}"#));
    let assessment = assess_origin(fingerprint(KEY_7), 1_000, &trust);
    assert_eq!(assessment, OriginAssessment::TrustedCollector { name: "example".to_string(), fingerprint: fingerprint(KEY_7) });
}

#[test]
fn another_key_is_not_trusted_because_someone_else_is() {
    let trust = list(&format!(r#"{{"name":"example","fingerprint":"{KEY_7}"}}"#));
    let other = [9u8; 32];
    assert!(matches!(assess_origin(other, 1_000, &trust), OriginAssessment::SelfAttested { .. }));
}

#[test]
fn a_revoked_collector_is_never_trusted() {
    let trust = list(&format!(r#"{{"name":"example","fingerprint":"{KEY_7}","revoked":true}}"#));
    assert!(matches!(assess_origin(fingerprint(KEY_7), 1_000, &trust), OriginAssessment::Revoked { .. }));
}

#[test]
fn a_proof_outside_the_collectors_validity_window_is_not_trusted() {
    let trust = list(&format!(r#"{{"name":"example","fingerprint":"{KEY_7}","validFromMs":1000,"validUntilMs":2000}}"#));
    assert!(matches!(assess_origin(fingerprint(KEY_7), 999, &trust), OriginAssessment::OutsideValidity { .. }));
    assert!(matches!(assess_origin(fingerprint(KEY_7), 2001, &trust), OriginAssessment::OutsideValidity { .. }));
    assert!(assess_origin(fingerprint(KEY_7), 1000, &trust).is_trusted());
    assert!(assess_origin(fingerprint(KEY_7), 2000, &trust).is_trusted());
}

#[test]
fn a_malformed_trust_list_is_refused_rather_than_read_as_empty() {
    assert!(matches!(TrustList::from_json("not json"), Err(TrustListError::Json(_))));
    assert_eq!(TrustList::from_json(r#"{"version":2,"collectors":[]}"#), Err(TrustListError::UnsupportedVersion(2)));
    assert!(matches!(
        TrustList::from_json(r#"{"version":1,"collectors":[{"name":"x","fingerprint":"abc"}]}"#),
        Err(TrustListError::BadFingerprint { .. })
    ));
    assert!(matches!(
        TrustList::from_json(&format!(r#"{{"version":1,"collectors":[{{"name":"x","fingerprint":"{KEY_7}","validFromMs":5,"validUntilMs":1}}]}}"#)),
        Err(TrustListError::EmptyValidity { .. })
    ));
    let twice = format!(r#"{{"name":"a","fingerprint":"{KEY_7}"}},{{"name":"b","fingerprint":"{KEY_7}"}}"#);
    assert!(matches!(
        TrustList::from_json(&format!(r#"{{"version":1,"collectors":[{twice}]}}"#)),
        Err(TrustListError::Duplicate(_))
    ));
}
