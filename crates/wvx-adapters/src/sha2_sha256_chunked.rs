//! Adapter: sha2.sha256-chunked@1
//!
//! SHA-256 via 4 KiB chunked `update` — multi-impl path #3 (equal digests).

use sha2::{Digest, Sha256};

const CHUNK: usize = 4096;

/// SHA-256 digest with fixed-size chunked updates.
pub fn digest(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut hasher = Sha256::new();
    for chunk in bytes.chunks(CHUNK) {
        hasher.update(chunk);
    }
    Ok(hasher.finalize().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha2_sha256;

    #[test]
    fn matches_oneshot() {
        let msg = vec![0xABu8; 10_000];
        assert_eq!(digest(&msg).unwrap(), sha2_sha256::digest(&msg).unwrap());
    }
}
