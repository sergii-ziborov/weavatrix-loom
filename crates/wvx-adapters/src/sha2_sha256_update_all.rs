//! Adapter: sha2.sha256-update-all@1
//!
//! SHA-256 via a single `update` of the full slice + `finalize` (not `Digest::digest`).
//! Multi-impl path #4 — same digests, different call shape.

use sha2::{Digest, Sha256};

/// SHA-256 digest via `update(full) + finalize`.
pub fn digest(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hasher.finalize().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha2_sha256;

    #[test]
    fn matches_oneshot() {
        for msg in [b"" as &[u8], b"hello", &[0u8; 64], &[1u8; 1024]] {
            assert_eq!(digest(msg).unwrap(), sha2_sha256::digest(msg).unwrap());
        }
    }
}
