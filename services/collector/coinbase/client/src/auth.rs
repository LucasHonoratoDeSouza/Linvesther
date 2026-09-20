//! Coinbase Developer Platform API keys: every request carries a fresh
//! ES256 JWT that names the exact method, host and path it is valid for,
//! so a captured token can't be replayed against anything else — and
//! expires two minutes after it is made.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use p256::pkcs8::DecodePrivateKey;
use p256::SecretKey;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("the private key is not a valid EC (P-256) key: {0}")]
    InvalidKey(String),
    #[error("the API key name is empty")]
    EmptyKeyName,
}

/// A CDP API key: its name (`organizations/…/apiKeys/…`) and its EC
/// private key, exactly as Coinbase's downloaded key file gives them.
#[derive(Clone)]
pub struct Credentials {
    key_name: String,
    signing_key: Signing,
}

#[derive(Clone)]
enum Signing {
    Ecdsa(SigningKey),
    Ed25519(ed25519_dalek::SigningKey),
}

impl std::fmt::Debug for Credentials {
    // Never prints key material.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials").field("key_name", &self.key_name).finish_non_exhaustive()
    }
}

/// People paste the key from the downloaded JSON, where the PEM is a
/// single line with literal `\n` sequences, or from a PEM file with real
/// newlines, or with the header lines missing — all of which mean the
/// same key.
fn normalize_pem(raw: &str) -> String {
    let text = raw.trim().trim_matches('"').replace("\\n", "\n").replace("\r\n", "\n");
    if text.contains("-----BEGIN") {
        return text;
    }
    // Header-less base64 body: told apart by its DER shape — a SEC1 EC key
    // is `SEQUENCE { INTEGER 1, OCTET STRING … }`, a PKCS#8 key
    // `SEQUENCE { INTEGER 0, SEQUENCE … }`.
    let body: String = text.split_whitespace().collect();
    let label = if is_sec1_der(&body) { "EC PRIVATE KEY" } else { "PRIVATE KEY" };
    let mut wrapped = format!("-----BEGIN {label}-----\n");
    for chunk in body.as_bytes().chunks(64) {
        wrapped.push_str(std::str::from_utf8(chunk).unwrap_or_default());
        wrapped.push('\n');
    }
    wrapped.push_str(&format!("-----END {label}-----\n"));
    wrapped
}

fn is_sec1_der(base64_body: &str) -> bool {
    let Ok(der) = base64::engine::general_purpose::STANDARD.decode(base64_body) else {
        return false;
    };
    // SEQUENCE tag, then a 1- or 2-byte length, then INTEGER 1.
    let after_header = match der.get(1) {
        Some(0x81) => 3,
        Some(0x82) => 4,
        Some(_) => 2,
        None => return false,
    };
    der.first() == Some(&0x30) && der.get(after_header..after_header + 3) == Some(&[0x02, 0x01, 0x01])
}

impl Credentials {
    pub fn new(key_name: &str, private_key_pem: &str) -> Result<Self, AuthError> {
        let key_name = key_name.trim().trim_matches('"').to_string();
        if key_name.is_empty() {
            return Err(AuthError::EmptyKeyName);
        }
        // "Secret" keys (Ed25519) come as a bare base64 string of 32 or 64 bytes.
        let bare: String = private_key_pem.trim().trim_matches('"').split_whitespace().collect();
        if !bare.contains("BEGIN") && !bare.contains("\\n") {
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&bare) {
                if bytes.len() == 32 || bytes.len() == 64 {
                    let seed: [u8; 32] = bytes[..32].try_into().expect("length checked");
                    return Ok(Credentials { key_name, signing_key: Signing::Ed25519(ed25519_dalek::SigningKey::from_bytes(&seed)) });
                }
            }
        }
        let pem = normalize_pem(private_key_pem);
        let secret = if pem.contains("BEGIN EC PRIVATE KEY") {
            SecretKey::from_sec1_pem(&pem).map_err(|e| AuthError::InvalidKey(e.to_string()))?
        } else {
            SecretKey::from_pkcs8_pem(&pem).map_err(|e| AuthError::InvalidKey(e.to_string()))?
        };
        Ok(Credentials { key_name, signing_key: Signing::Ecdsa(SigningKey::from(secret)) })
    }

    pub fn key_name(&self) -> &str {
        &self.key_name
    }

    /// A JWT valid for exactly `method` `host``path` (no query string —
    /// Coinbase signs the path alone) for the next two minutes.
    pub fn jwt(&self, method: &str, host: &str, path: &str, now_secs: u64, nonce_hex: &str) -> String {
        let alg = match self.signing_key {
            Signing::Ecdsa(_) => "ES256",
            Signing::Ed25519(_) => "EdDSA",
        };
        let header = serde_json::json!({ "alg": alg, "kid": self.key_name, "nonce": nonce_hex, "typ": "JWT" });
        let claims = serde_json::json!({
            "sub": self.key_name,
            "iss": "cdp",
            "nbf": now_secs,
            "exp": now_secs + 120,
            "uri": format!("{method} {host}{path}"),
        });
        let signing_input = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(header.to_string()),
            URL_SAFE_NO_PAD.encode(claims.to_string())
        );
        let signature = match &self.signing_key {
            Signing::Ecdsa(key) => {
                let signature: Signature = key.sign(signing_input.as_bytes());
                signature.to_bytes().to_vec()
            }
            Signing::Ed25519(key) => ed25519_dalek::Signer::sign(key, signing_input.as_bytes()).to_bytes().to_vec(),
        };
        format!("{signing_input}.{}", URL_SAFE_NO_PAD.encode(signature))
    }
}

/// 16 random bytes as hex, for the JWT `nonce` header.
pub fn random_nonce() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("the operating system provides randomness");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::{signature::Verifier, VerifyingKey};
    use p256::pkcs8::EncodePrivateKey;

    /// A throwaway key generated for these tests only.
    fn test_secret() -> SecretKey {
        SecretKey::from_slice(&[7u8; 32]).unwrap()
    }
    fn sec1_pem() -> String {
        test_secret().to_sec1_pem(Default::default()).unwrap().to_string()
    }
    fn pkcs8_pem() -> String {
        test_secret().to_pkcs8_pem(Default::default()).unwrap().to_string()
    }

    #[test]
    fn an_ed25519_secret_key_signs_a_jwt_that_verifies() {
        use ed25519_dalek::Verifier as _;
        let seed = [9u8; 32];
        let public = ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key();
        let mut full = seed.to_vec();
        full.extend_from_slice(public.as_bytes());
        let secret = base64::engine::general_purpose::STANDARD.encode(&full);
        let credentials = Credentials::new("eace7f97-c808", &secret).unwrap();
        let token = credentials.jwt("GET", "api.coinbase.com", "/x", 1_000, "ab");
        let parts: Vec<&str> = token.split('.').collect();
        let header: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        assert_eq!(header["alg"], "EdDSA");
        assert_eq!(header["kid"], "eace7f97-c808");
        let signature = ed25519_dalek::Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).unwrap();
        assert!(public.verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature).is_ok());
    }

    #[test]
    fn the_jwt_names_exactly_the_request_it_is_for_and_verifies_with_the_public_key() {
        let credentials = Credentials::new("organizations/o/apiKeys/k", &sec1_pem()).unwrap();
        let token = credentials.jwt("GET", "api.coinbase.com", "/api/v3/brokerage/accounts", 1_000, "ab12");
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);

        let header: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        assert_eq!(header["alg"], "ES256");
        assert_eq!(header["kid"], "organizations/o/apiKeys/k");
        assert_eq!(header["nonce"], "ab12");

        let claims: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["iss"], "cdp");
        assert_eq!(claims["sub"], "organizations/o/apiKeys/k");
        assert_eq!(claims["uri"], "GET api.coinbase.com/api/v3/brokerage/accounts");
        assert_eq!(claims["nbf"], 1_000);
        assert_eq!(claims["exp"], 1_120);

        let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).unwrap();
        let verifying = VerifyingKey::from(&test_secret().public_key());
        assert!(verifying.verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature).is_ok());
    }

    #[test]
    fn a_pasted_key_works_in_every_shape_people_actually_paste() {
        let reference = Credentials::new("k", &sec1_pem()).unwrap().jwt("GET", "h", "/p", 1, "n");
        // ECDSA signatures are randomised, so compare what they verify, not their bytes.
        let verifying = VerifyingKey::from(&test_secret().public_key());
        let verifies = |token: &str| {
            let parts: Vec<&str> = token.split('.').collect();
            let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).unwrap();
            verifying.verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature).is_ok()
        };
        assert!(verifies(&reference));

        let escaped = sec1_pem().trim().replace('\n', "\\n"); // the JSON file's single-line form
        assert!(verifies(&Credentials::new("k", &escaped).unwrap().jwt("GET", "h", "/p", 1, "n")));
        assert!(verifies(&Credentials::new("k", &format!("\"{escaped}\"")).unwrap().jwt("GET", "h", "/p", 1, "n")));
        assert!(verifies(&Credentials::new("k", &pkcs8_pem()).unwrap().jwt("GET", "h", "/p", 1, "n")));
        let bare: String = sec1_pem().lines().filter(|l| !l.starts_with("-----")).collect();
        assert!(verifies(&Credentials::new("k", &bare).unwrap().jwt("GET", "h", "/p", 1, "n")));
    }

    #[test]
    fn nonsense_is_refused_with_a_clear_reason_and_never_echoes_key_material() {
        assert_eq!(Credentials::new("  ", &sec1_pem()).unwrap_err(), AuthError::EmptyKeyName);
        assert!(matches!(Credentials::new("k", "not a key"), Err(AuthError::InvalidKey(_))));
        let debug = format!("{:?}", Credentials::new("k", &sec1_pem()).unwrap());
        assert!(debug.contains("key_name") && !debug.contains("signing_key"));
    }

    #[test]
    fn nonces_are_random_hex() {
        let (a, b) = (random_nonce(), random_nonce());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
