//! CSPRNG salt generation for commitments.
//!
//! Per the protocol specification: "Salt vem de CSPRNG... nunca é
//! derivado apenas de saldo, UID ou senha." This module only ever draws
//! from the OS CSPRNG (via `getrandom`, the same primitive `rand`'s
//! `OsRng` uses) — there is no constructor that derives a salt from
//! caller-supplied data, so that misuse is not merely discouraged but
//! structurally unavailable through this API.

#[derive(Debug, thiserror::Error)]
#[error("failed to read from the OS CSPRNG: {0}")]
pub struct SaltError(getrandom::Error);

/// Draws a fresh 256-bit salt from the OS CSPRNG.
pub fn generate_salt() -> Result<[u8; 32], SaltError> {
    let mut salt = [0u8; 32];
    getrandom::getrandom(&mut salt).map_err(SaltError)?;
    Ok(salt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_draws_are_not_equal() {
        // Not a proof of entropy quality, but catches a broken/stubbed
        // generator (e.g. an all-zero fallback) immediately.
        let a = generate_salt().unwrap();
        let b = generate_salt().unwrap();
        assert_ne!(a, b);
    }
}
