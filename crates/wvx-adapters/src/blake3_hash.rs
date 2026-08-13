//! Adapter: blake3.blake3@1
//!
//! BLAKE3 digest (32 bytes) via the `blake3` crate — second algorithm in the
//! hash domain (not a second SHA-256 backend).

/// BLAKE3 keyed-less hash of `bytes` (32 raw bytes).
pub fn digest(bytes: &[u8]) -> Result<Vec<u8>, String> {
    Ok(blake3::hash(bytes).as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_empty_stable() {
        let a = digest(b"loom").unwrap();
        let b = digest(b"loom").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 32);
        // Different from SHA-256 of same message
        let sha = crate::sha2_sha256::digest(b"loom").unwrap();
        assert_ne!(a, sha);
    }
}
