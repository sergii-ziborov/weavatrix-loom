//! Adapter: blake3.blake3-parallel@1
//!
//! Same 32-byte BLAKE3 digest as [`crate::blake3_hash`], hashed with
//! `Hasher::update_rayon` so large inputs can use multiple cores.

/// BLAKE3 digest via rayon-parallel `update_rayon` (must match oneshot).
pub fn digest(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_rayon(bytes);
    Ok(hasher.finalize().as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_oneshot_small_and_bulk() {
        for input in [b"".as_slice(), b"loom", &[b'a'; 64 * 1024]] {
            assert_eq!(
                digest(input).unwrap(),
                crate::blake3_hash::digest(input).unwrap()
            );
        }
    }
}
