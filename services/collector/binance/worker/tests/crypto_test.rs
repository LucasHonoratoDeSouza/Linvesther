use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use binance_worker::crypto::{credential_context, decrypt, encrypt, is_current, MasterKey};

fn key() -> MasterKey {
    MasterKey::from_hex(&"ab".repeat(32)).unwrap() // 32 bytes, exactly 64 hex chars
}

fn ctx(account: &str) -> String {
    credential_context("binance", account, "api_key")
}

/// What the earlier format stored: raw AES-GCM output, no marker, no context.
fn legacy_encrypt(key_hex: &str, plaintext: &str) -> (Vec<u8>, [u8; 12]) {
    let key = hex::decode(key_hex).unwrap();
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut nonce).unwrap();
    let ciphertext = Aes256Gcm::new_from_slice(&key).unwrap().encrypt(&Nonce::from(nonce), Payload { msg: plaintext.as_bytes(), aad: b"" }).unwrap();
    (ciphertext, nonce)
}

#[test]
fn round_trips_exactly_with_the_correct_key_and_account() {
    let k = key();
    let encrypted = encrypt(&k, "sk-live-deadbeef", &ctx("0xaaa")).unwrap();
    assert_eq!(decrypt(&k, &encrypted.ciphertext, &encrypted.nonce, &ctx("0xaaa")).unwrap(), "sk-live-deadbeef");
}

#[test]
fn the_ciphertext_never_contains_the_plaintext() {
    let encrypted = encrypt(&key(), "sk-live-deadbeef", &ctx("0xaaa")).unwrap();
    assert!(!encrypted.ciphertext.windows(4).any(|w| w == b"sk-l"));
}

#[test]
fn the_wrong_key_fails_to_decrypt() {
    let encrypted = encrypt(&key(), "sk-live-deadbeef", &ctx("0xaaa")).unwrap();
    let wrong_key = MasterKey::from_hex(&"cd".repeat(32)).unwrap();
    assert!(decrypt(&wrong_key, &encrypted.ciphertext, &encrypted.nonce, &ctx("0xaaa")).is_err());
}

#[test]
fn a_tampered_ciphertext_fails_to_decrypt() {
    let k = key();
    let mut encrypted = encrypt(&k, "sk-live-deadbeef", &ctx("0xaaa")).unwrap();
    let last = encrypted.ciphertext.len() - 1;
    encrypted.ciphertext[last] ^= 0xff;
    assert!(decrypt(&k, &encrypted.ciphertext, &encrypted.nonce, &ctx("0xaaa")).is_err());
}

#[test]
fn a_ciphertext_moved_to_another_account_does_not_decrypt_there() {
    let k = key();
    let encrypted = encrypt(&k, "sk-live-deadbeef", &ctx("0xaaa")).unwrap();
    assert!(decrypt(&k, &encrypted.ciphertext, &encrypted.nonce, &ctx("0xbbb")).is_err(), "another account");
    assert!(decrypt(&k, &encrypted.ciphertext, &encrypted.nonce, &credential_context("binance", "0xaaa", "api_secret")).is_err(), "another field");
    assert!(decrypt(&k, &encrypted.ciphertext, &encrypted.nonce, &credential_context("coinbase", "0xaaa", "api_key")).is_err(), "another broker");
}

#[test]
fn values_written_by_the_earlier_format_still_decrypt() {
    let hex_key = "ab".repeat(32);
    let (ciphertext, nonce) = legacy_encrypt(&hex_key, "sk-from-before");
    assert_eq!(decrypt(&key(), &ciphertext, &nonce, &ctx("0xaaa")).unwrap(), "sk-from-before");
}

#[test]
fn an_earlier_format_value_that_begins_with_the_current_marker_still_decrypts() {
    let hex_key = "ab".repeat(32);
    // The marker is one byte, so about one in 256 earlier values start with it by chance.
    let (ciphertext, nonce) = (0..20_000).map(|_| legacy_encrypt(&hex_key, "sk-from-before")).find(|(ct, _)| ct[0] == 2).expect("a value beginning with the marker");
    assert_eq!(decrypt(&key(), &ciphertext, &nonce, &ctx("0xaaa")).unwrap(), "sk-from-before");
}

#[test]
fn data_written_under_a_previous_key_is_read_during_rotation_and_not_after() {
    let old = key();
    let encrypted = encrypt(&old, "sk-live-deadbeef", &ctx("0xaaa")).unwrap();
    let new_key_hex = "cd".repeat(32);

    let rotating = MasterKey::from_hex(&new_key_hex).unwrap().with_previous(&"ab".repeat(32)).unwrap();
    assert_eq!(decrypt(&rotating, &encrypted.ciphertext, &encrypted.nonce, &ctx("0xaaa")).unwrap(), "sk-live-deadbeef");
    assert!(!is_current(&rotating, &encrypted.ciphertext, &encrypted.nonce, &ctx("0xaaa")), "it still needs re-encrypting");

    let after = MasterKey::from_hex(&new_key_hex).unwrap();
    assert!(decrypt(&after, &encrypted.ciphertext, &encrypted.nonce, &ctx("0xaaa")).is_err(), "once the old key is dropped it cannot be read");
}

#[test]
fn what_is_written_now_is_current_and_what_came_before_is_not() {
    let k = key();
    let now = encrypt(&k, "x", &ctx("0xaaa")).unwrap();
    assert!(is_current(&k, &now.ciphertext, &now.nonce, &ctx("0xaaa")));
    assert!(!is_current(&k, &now.ciphertext, &now.nonce, &ctx("0xbbb")), "not for another account");
    let (legacy, nonce) = legacy_encrypt(&"ab".repeat(32), "x");
    assert!(!is_current(&k, &legacy, &nonce, &ctx("0xaaa")));
}

#[test]
fn rejects_a_master_key_of_the_wrong_length() {
    assert!(MasterKey::from_hex("deadbeef").is_err());
    assert!(key().with_previous("deadbeef").is_err());
}

#[test]
fn rejects_non_hex_input() {
    assert!(MasterKey::from_hex("not-hex-at-all-not-hex-at-all-not-hex-at-all-not-hex-at-allxx").is_err());
}
