//! Keeping stored credentials in the current format under the current key.
//!
//! Credentials written before a secret was bound to its account, or under a key
//! that has since been rotated, are still read (see `crypto::decrypt`). `rekey`
//! re-encrypts them; `check` reports what would change without changing it.
//! Neither ever prints or logs a secret: only counts and account ids.

use crate::crypto::{credential_context, decrypt, encrypt, is_current, MasterKey};
use serde::Serialize;
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// One stored secret: its two columns and the field name it is bound to.
struct Field {
    ciphertext: &'static str,
    nonce: &'static str,
    name: &'static str,
}

struct Table {
    broker: &'static str,
    table: &'static str,
    fields: &'static [Field],
}

const TABLES: [Table; 5] = [
    Table {
        broker: "binance",
        table: "binance_connections",
        fields: &[
            Field { ciphertext: "encrypted_api_key", nonce: "nonce_api_key", name: "api_key" },
            Field { ciphertext: "encrypted_api_secret", nonce: "nonce_api_secret", name: "api_secret" },
        ],
    },
    Table {
        broker: "coinbase",
        table: "coinbase_connections",
        fields: &[
            Field { ciphertext: "encrypted_key_name", nonce: "nonce_key_name", name: "key_name" },
            Field { ciphertext: "encrypted_private_key", nonce: "nonce_private_key", name: "private_key" },
        ],
    },
    Table {
        broker: "ibkr",
        table: "ibkr_connections",
        fields: &[
            Field { ciphertext: "encrypted_token", nonce: "nonce_token", name: "token" },
            Field { ciphertext: "encrypted_query_id", nonce: "nonce_query_id", name: "query_id" },
        ],
    },
    Table {
        broker: "kraken",
        table: "kraken_connections",
        fields: &[
            Field { ciphertext: "encrypted_api_key", nonce: "nonce_api_key", name: "api_key" },
            Field { ciphertext: "encrypted_api_secret", nonce: "nonce_api_secret", name: "api_secret" },
        ],
    },
    Table {
        broker: "wallet",
        table: "wallet_connections",
        fields: &[Field { ciphertext: "encrypted_address", nonce: "nonce_address", name: "address" }],
    },
];

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct RekeyReport {
    /// Connections examined.
    pub scanned: usize,
    /// Connections re-encrypted (or, in a check, that would be).
    pub upgraded: usize,
    /// Connections already in the current format under the current key.
    #[serde(rename = "alreadyCurrent")]
    pub already_current: usize,
    /// Accounts whose credential could not be decrypted with any known key.
    /// They are left exactly as they were.
    pub unreadable: Vec<String>,
}

fn nonce_of(bytes: Vec<u8>) -> Option<[u8; 12]> {
    bytes.try_into().ok()
}

async fn run(pool: &PgPool, key: &MasterKey, write: bool) -> Result<RekeyReport, sqlx::Error> {
    let mut report = RekeyReport::default();
    for table in &TABLES {
        let columns: Vec<String> = table.fields.iter().flat_map(|f| [f.ciphertext.to_string(), f.nonce.to_string()]).collect();
        let select = format!("SELECT id, account_id, {} FROM {}", columns.join(", "), table.table);
        let assignments: Vec<String> = table.fields.iter().enumerate().flat_map(|(i, f)| [format!("{} = ${}", f.ciphertext, 2 * i + 1), format!("{} = ${}", f.nonce, 2 * i + 2)]).collect();
        let update = format!("UPDATE {} SET {} WHERE id = ${}", table.table, assignments.join(", "), 2 * table.fields.len() + 1);
        for row in sqlx::query(&select).fetch_all(pool).await? {
            report.scanned += 1;
            let id: Uuid = row.get("id");
            let account_id: String = row.get("account_id");
            let mut fresh: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(table.fields.len());
            let mut needs_update = false;
            let mut readable = true;
            for field in table.fields {
                let ciphertext: Vec<u8> = row.get(field.ciphertext);
                let nonce = nonce_of(row.get(field.nonce));
                let context = credential_context(table.broker, &account_id, field.name);
                match nonce.and_then(|n| decrypt(key, &ciphertext, &n, &context).ok().map(|plain| (n, plain))) {
                    None => {
                        readable = false;
                        break;
                    }
                    Some((n, plain)) => {
                        if is_current(key, &ciphertext, &n, &context) {
                            fresh.push((ciphertext, n.to_vec()));
                        } else {
                            needs_update = true;
                            match encrypt(key, &plain, &context) {
                                Ok(encrypted) => fresh.push((encrypted.ciphertext, encrypted.nonce.to_vec())),
                                Err(_) => {
                                    readable = false;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            if !readable {
                report.unreadable.push(account_id);
            } else if !needs_update {
                report.already_current += 1;
            } else {
                if write {
                    let mut query = sqlx::query(&update);
                    for (ciphertext, nonce) in &fresh {
                        query = query.bind(ciphertext).bind(nonce);
                    }
                    query.bind(id).execute(pool).await?;
                }
                report.upgraded += 1;
            }
        }
    }
    Ok(report)
}

/// Re-encrypts every stored credential that is not yet current.
pub async fn rekey(pool: &PgPool, key: &MasterKey) -> Result<RekeyReport, sqlx::Error> {
    run(pool, key, true).await
}

/// The same report, changing nothing.
pub async fn check(pool: &PgPool, key: &MasterKey) -> Result<RekeyReport, sqlx::Error> {
    run(pool, key, false).await
}
