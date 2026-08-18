//! Adapter: sha2.sha256-streaming@1
//!
//! Same SHA-256 contract as `sha2.sha256@1`, but updates the hasher **one byte
//! at a time**. Proves multi-impl swap: different code path, equal digests
//! (Gate A style equality for Domain 2).

use sha2::{Digest, Sha256};

/// SHA-256 digest via streaming `update` (must match one-shot for all inputs).
pub fn digest(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut hasher = Sha256::new();
    for b in bytes {
        hasher.update([*b]);
    }
    Ok(hasher.finalize().to_vec())
}

/// I/O streaming (64 KiB). Slice API above stays one-byte for multi-impl equality.
pub fn digest_read<R: std::io::Read>(reader: R) -> Result<Vec<u8>, String> {
    let mut hasher = Sha256::new();
    crate::stream::pump(reader, |chunk| {
        hasher.update(chunk);
        Ok(())
    })?;
    Ok(hasher.finalize().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha2_sha256;

    #[test]
    fn matches_oneshot() {
        for msg in [b"" as &[u8], b"hello", b"Weavatrix Loom Domain 2"] {
            assert_eq!(digest(msg).unwrap(), sha2_sha256::digest(msg).unwrap());
            assert_eq!(digest_read(msg).unwrap(), digest(msg).unwrap());
        }
    }
}
