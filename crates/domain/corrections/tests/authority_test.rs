use corrections::{is_admissible, Authority, Correction, OwnerObjection};

fn admissible_correction() -> Correction {
    Correction {
        target_digest: [1u8; 32],
        replacement_digest: [2u8; 32],
        reason_code: "bad-price-feed".to_string(),
        evidence_digest: [7u8; 32],
        authority: Authority::FinancialCorrection,
    }
}

#[test]
fn admissible_correction_holds_with_no_objection() {
    assert!(is_admissible(&admissible_correction(), None));
}

#[test]
fn owner_objection_does_not_veto_validated_evidence() {
    let correction = admissible_correction();
    let objection = OwnerObjection {
        reason: "I disagree with this number".to_string(),
    };

    let without_objection = is_admissible(&correction, None);
    let with_objection = is_admissible(&correction, Some(&objection));

    assert_eq!(
        without_objection, with_objection,
        "the owner's objection must not change admissibility"
    );
    assert!(with_objection);
}

#[test]
fn owner_signature_alone_cannot_manufacture_admissibility_either() {
    // A correction with no evidence is inadmissible regardless of what
    // the owner says about it — the owner cannot force a favorable
    // number through any more than they can block an unfavorable one.
    let mut correction = admissible_correction();
    correction.evidence_digest = [0u8; 32];
    let objection = OwnerObjection {
        reason: "please accept this anyway".to_string(),
    };
    assert!(!is_admissible(&correction, Some(&objection)));
}
