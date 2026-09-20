use binance_conformance::signing::sign_query;

#[test]
fn signs_documented_binance_example_vector() {
    // Fixture query/secret/signature triple from Binance's own REST API
    // documentation (SIGNED endpoint security example).
    let secret = "NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j";
    let query = "symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559";
    let expected = "c8db56825ae71d6d79447849e617115f4a920fa2acdcab2b053c4b2838bd6b71";

    let signature =
        sign_query(secret, query).expect("signing must succeed with a non-empty secret");

    assert_eq!(signature, expected);
}

#[test]
fn rejects_empty_secret() {
    let result = sign_query("", "symbol=BTCUSDT");
    assert!(result.is_err());
}

#[test]
fn same_query_different_secret_yields_different_signature() {
    let query = "symbol=BTCUSDT&timestamp=1";
    let a = sign_query("secret-a", query).unwrap();
    let b = sign_query("secret-b", query).unwrap();
    assert_ne!(a, b);
}
