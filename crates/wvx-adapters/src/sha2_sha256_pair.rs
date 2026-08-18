//! Adapter: sha2.sha256-pair@1
//!
//! Multi-output Domain 2 impl: raw digest **and** lowercase hex (same SHA-256).

/// `(digest, hex)` — 32 raw bytes and 64 ASCII hex bytes.
pub fn digest_hex(bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let digest = crate::sha2_sha256::digest(bytes)?;
    let hex = crate::reference_hex_encode::encode(&digest)?;
    Ok((digest, hex))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_and_hex() {
        let (d, h) = digest_hex(b"").unwrap();
        assert_eq!(d.len(), 32);
        assert_eq!(
            h,
            b"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(h, crate::reference_hex_encode::encode(&d).unwrap());
    }
}
