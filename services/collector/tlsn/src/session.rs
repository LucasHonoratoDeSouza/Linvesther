//! TLS origin session receipt (POC), per the protocol specification's A1
//! requirements: "A1 deve demonstrar hostname/
//! certificado, método/path/query/body, headers relevantes, status,
//! corpo integral necessário à interpretação, paginação e vínculo de
//! credencial constante entre endpoints. Redaction NÃO DEVE ocultar
//! seletor de carteira, símbolo, filtro, tipo, janela temporal, erro ou
//! contagem relevante."
//!
//! This is explicitly a POC, per this task's own "Done when": it
//! registers what it does and does not homologate, rather than
//! claiming a full TLSNotary integration — see this crate's README for
//! the limits.

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsSessionReceipt {
    pub server_name: String,
    pub request_method: String,
    pub request_path: String,
    pub request_query: String,
    pub response_status: u16,
    /// SHA-256 of the full response body actually captured in the
    /// session — the same digest downstream calculation must trace
    /// back to (see `verify_raw_bound_to_input`).
    pub response_body_hash: [u8; 32],
    /// Fields the session explicitly discloses (not redacted). Per
    /// proofs.md, redaction must never hide these categories.
    pub disclosed_fields: Vec<String>,
    /// A signature over this receipt's own digest, produced by the
    /// wallet the session is bound to.
    pub wallet_binding_signature: Vec<u8>,
    pub wallet_address: [u8; 20],
}

impl TlsSessionReceipt {
    /// The digest the wallet-binding signature covers: every field
    /// except the signature itself, so a session detail changed after
    /// signing invalidates the binding.
    pub fn binding_digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.server_name.as_bytes());
        hasher.update(self.request_method.as_bytes());
        hasher.update(self.request_path.as_bytes());
        hasher.update(self.request_query.as_bytes());
        hasher.update(self.response_status.to_be_bytes());
        hasher.update(self.response_body_hash);
        for field in &self.disclosed_fields {
            hasher.update(field.as_bytes());
        }
        hasher.update(self.wallet_address);
        hasher.finalize().into()
    }
}
