use collector_attestation::{origin_badge, OriginBadge};

#[test]
fn an_a0_mechanism_yields_an_a0_badge() {
    assert_eq!(origin_badge("A0"), OriginBadge::A0);
}

#[test]
fn an_unrecognized_mechanism_never_defaults_to_something_stronger_than_a0() {
    assert_eq!(origin_badge("bridge-attested"), OriginBadge::A0);
    assert_eq!(origin_badge(""), OriginBadge::A0);
}

#[test]
fn a1_and_a2_mechanisms_are_reported_as_declared() {
    assert_eq!(origin_badge("A1"), OriginBadge::A1);
    assert_eq!(origin_badge("A2"), OriginBadge::A2);
}
