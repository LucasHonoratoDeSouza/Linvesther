//! Request signing confined to a fixed host and an explicit path
//! allowlist.

use collector_credentials::{host_for, sign_request, Environment, SigningError};

#[test]
fn signs_documented_binance_example_vector() {
    // Same fixture as experiments/binance-conformance: Binance's own REST
    // API documentation SIGNED endpoint example.
    let secret = "NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j";
    let query = "symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559";
    let expected = "c8db56825ae71d6d79447849e617115f4a920fa2acdcab2b053c4b2838bd6b71";

    let signature = sign_request(secret, "/api/v3/account", query).unwrap();
    assert_eq!(signature, expected);
}

#[test]
fn rejects_path_outside_the_allowlist() {
    let result = sign_request("secret", "/sapi/v1/margin/borrow", "symbol=BTCUSDT");
    assert_eq!(
        result,
        Err(SigningError::PathNotAllowed(
            "/sapi/v1/margin/borrow".to_string()
        ))
    );
}

#[test]
fn rejects_empty_secret() {
    let result = sign_request("", "/api/v3/account", "symbol=BTCUSDT");
    assert_eq!(result, Err(SigningError::EmptySecret));
}

#[test]
fn every_allowlisted_path_is_signable() {
    for path in collector_credentials::signing::ALLOWED_PATHS {
        assert!(
            sign_request("secret", path, "x=1").is_ok(),
            "path {path} should be signable"
        );
    }
}

#[test]
fn host_is_fixed_per_environment_not_caller_supplied() {
    assert_eq!(host_for(Environment::Production), "https://api.binance.com");
    assert_eq!(
        host_for(Environment::Test),
        "https://testnet.binance.vision"
    );
}
