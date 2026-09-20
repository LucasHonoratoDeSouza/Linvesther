use binance_worker::crypto::{decrypt, encrypt, MasterKey};

fn key() -> MasterKey {
    let hex_key = "ab".repeat(32); // 32 bytes, exactly 64 hex chars
    MasterKey::from_hex(&hex_key).unwrap()
}

#[test]
fn round_trips_exactly_with_the_correct_key() {
    let k = key();
    let encrypted = encrypt(&k, "sk-live-deadbeef").unwrap();
    let decrypted = decrypt(&k, &encrypted.ciphertext, &encrypted.nonce).unwrap();
    assert_eq!(decrypted, "sk-live-deadbeef");
}

#[test]
fn the_ciphertext_never_contains_the_plaintext() {
    let k = key();
    let encrypted = encrypt(&k, "sk-live-deadbeef").unwrap();
    assert!(!encrypted.ciphertext.windows(4).any(|w| w == b"sk-l"));
}

#[test]
fn the_wrong_key_fails_to_decrypt() {
    let encrypted = encrypt(&key(), "sk-live-deadbeef").unwrap();
    let wrong_key = MasterKey::from_hex(&"cd".repeat(32)).unwrap();
    assert!(decrypt(&wrong_key, &encrypted.ciphertext, &encrypted.nonce).is_err());
}

#[test]
fn a_tampered_ciphertext_fails_to_decrypt() {
    let k = key();
    let mut encrypted = encrypt(&k, "sk-live-deadbeef").unwrap();
    encrypted.ciphertext[0] ^= 0xff;
    assert!(decrypt(&k, &encrypted.ciphertext, &encrypted.nonce).is_err());
}

#[test]
fn rejects_a_master_key_of_the_wrong_length() {
    assert!(MasterKey::from_hex("deadbeef").is_err());
}

#[test]
fn rejects_non_hex_input() {
    assert!(
        MasterKey::from_hex("not-hex-at-all-not-hex-at-all-not-hex-at-all-not-hex-at-allxx")
            .is_err()
    );
}
