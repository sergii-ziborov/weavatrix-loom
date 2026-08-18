//! Adapter: sha2.sha256@1
//!
//! One-shot SHA-256 via the `sha2` crate (pure Rust).

use sha2::{Digest, Sha256};

/// SHA-256 digest of `bytes` (32 raw bytes).
pub fn digest(bytes: &[u8]) -> Result<Vec<u8>, String> {
    Ok(Sha256::digest(bytes).to_vec())
}

/// SHA-256 from a `Read` (64 KiB chunks). Equals [`digest`] on the same bytes.
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

    #[test]
    fn empty_known_vector() {
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let d = digest(b"").unwrap();
        assert_eq!(
            hex::encode_lower(&d),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(digest_read(&b""[..]).unwrap(), d);
    }
}

// tiny hex helper without extra dep
#[cfg(test)]
mod hex {
    pub fn encode_lower(bytes: &[u8]) -> String {
        const H: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push(H[(b >> 4) as usize] as char);
            s.push(H[(b & 0xf) as usize] as char);
        }
        s
    }
}
