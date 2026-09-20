//! TLS session receipt, mirroring
//! `services/collector/tlsn::session::TlsSessionReceipt` field-for-field
//! — `services/` and `zkvm/` are separate Cargo workspaces, so the
//! type cannot be shared directly; both are independent implementations
//! of the same the protocol specification A1 receipt shape.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsSessionReceipt {
    pub server_name: String,
    pub request_method: String,
    pub request_path: String,
    pub request_query: String,
    pub response_status: u16,
    pub response_body_hash: [u8; 32],
    pub disclosed_fields: Vec<String>,
    pub wallet_binding_signature: Vec<u8>,
    pub wallet_address: [u8; 20],
}

impl TlsSessionReceipt {
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
