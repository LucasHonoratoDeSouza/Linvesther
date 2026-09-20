use compute_jobs::{Effect, Outbox};

fn effect(key: &str) -> Effect {
    Effect {
        idempotency_key: key.to_string(),
        payload_digest: [1u8; 32],
    }
}

#[test]
fn crash_between_effects_is_resumed_without_duplication() {
    let steps = [effect("step-1"), effect("step-2"), effect("step-3")];
    let mut outbox = Outbox::new();

    // Applies the first two effects, then "crashes" before the third.
    for step in &steps[..2] {
        outbox.apply(step.clone());
    }
    assert_eq!(outbox.applied_effects().len(), 2);

    // Resumption re-runs the job from the start, replaying every step —
    // including the two already applied.
    for step in &steps {
        outbox.apply(step.clone());
    }

    assert_eq!(
        outbox.applied_effects().len(),
        3,
        "each idempotency key is recorded exactly once, no matter how many times it is replayed"
    );
}

#[test]
fn applying_a_new_effect_returns_true_and_a_repeat_returns_false() {
    let mut outbox = Outbox::new();
    assert!(outbox.apply(effect("only")));
    assert!(!outbox.apply(effect("only")));
}

#[test]
fn effects_with_different_keys_are_independent() {
    let mut outbox = Outbox::new();
    assert!(outbox.apply(effect("a")));
    assert!(outbox.apply(effect("b")));
    assert_eq!(outbox.applied_effects().len(), 2);
}
