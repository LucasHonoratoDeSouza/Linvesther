//! UID/sub-account/environment binding pinning and mismatch detection.

use collector_credentials::{AccountContext, BindingError, Environment, PinnedBinding};

fn context(uid: &str, sub: Option<&str>, env: Environment) -> AccountContext {
    AccountContext {
        uid: uid.to_string(),
        sub_account_id: sub.map(String::from),
        environment: env,
    }
}

#[test]
fn matching_context_passes() {
    let pinned = PinnedBinding::pin(context("uid-1", None, Environment::Production));
    assert!(pinned
        .check(&context("uid-1", None, Environment::Production))
        .is_ok());
}

#[test]
fn uid_change_is_rejected() {
    let pinned = PinnedBinding::pin(context("uid-1", None, Environment::Production));
    let result = pinned.check(&context("uid-2", None, Environment::Production));
    assert_eq!(
        result,
        Err(BindingError::UidMismatch {
            expected: "uid-1".to_string(),
            actual: "uid-2".to_string()
        })
    );
}

#[test]
fn environment_change_is_rejected_even_with_same_uid() {
    let pinned = PinnedBinding::pin(context("uid-1", None, Environment::Production));
    let result = pinned.check(&context("uid-1", None, Environment::Test));
    assert_eq!(
        result,
        Err(BindingError::EnvironmentMismatch {
            expected: Environment::Production,
            actual: Environment::Test
        })
    );
}

#[test]
fn subaccount_change_is_rejected() {
    let pinned = PinnedBinding::pin(context("uid-1", Some("sub-a"), Environment::Production));
    let result = pinned.check(&context("uid-1", Some("sub-b"), Environment::Production));
    assert_eq!(
        result,
        Err(BindingError::SubAccountMismatch {
            expected: Some("sub-a".to_string()),
            actual: Some("sub-b".to_string())
        })
    );
}

#[test]
fn root_account_to_subaccount_is_a_mismatch_not_a_wildcard() {
    let pinned = PinnedBinding::pin(context("uid-1", None, Environment::Production));
    let result = pinned.check(&context("uid-1", Some("sub-a"), Environment::Production));
    assert!(matches!(
        result,
        Err(BindingError::SubAccountMismatch { .. })
    ));
}

#[test]
fn uid_check_takes_priority_over_environment_and_subaccount() {
    // When multiple things differ, the UID mismatch (the more severe
    // finding — a different account entirely) is the one reported.
    let pinned = PinnedBinding::pin(context("uid-1", Some("sub-a"), Environment::Production));
    let result = pinned.check(&context("uid-2", Some("sub-b"), Environment::Test));
    assert!(matches!(result, Err(BindingError::UidMismatch { .. })));
}
