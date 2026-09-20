use corrections::{reconcile_conflicting, Resolution, Supersession};

#[test]
fn conflicting_versions_without_supersession_stay_disputed() {
    let a = [1u8; 32];
    let b = [2u8; 32];
    let resolution = reconcile_conflicting(a, b, None);
    assert_eq!(
        resolution,
        Resolution::Disputed { a, b },
        "both versions are preserved, neither is chosen as favorable"
    );
}

#[test]
fn demonstrated_supersession_resolves_in_favor_of_the_superseding_digest() {
    let a = [1u8; 32];
    let b = [2u8; 32];
    let resolution = reconcile_conflicting(a, b, Some(Supersession { superseding: b }));
    assert_eq!(resolution, Resolution::Resolved { winner: b });
}

#[test]
fn supersession_pointing_at_neither_version_is_treated_as_no_supersession() {
    let a = [1u8; 32];
    let b = [2u8; 32];
    let unrelated = Supersession {
        superseding: [9u8; 32],
    };
    let resolution = reconcile_conflicting(a, b, Some(unrelated));
    assert_eq!(resolution, Resolution::Disputed { a, b });
}
