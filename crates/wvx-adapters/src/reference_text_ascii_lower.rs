//! Adapter: wvx.reference.text-ascii-lower@1
//!
//! ASCII-only lowercasing; non-ASCII bytes left unchanged.

/// Lowercase A–Z only; leave other bytes unchanged.
pub fn transform(bytes: &[u8]) -> Result<Vec<u8>, String> {
    Ok(bytes
        .iter()
        .map(|b| if (b'A'..=b'Z').contains(b) { b + 32 } else { *b })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_only() {
        assert_eq!(transform(b"HELLO").unwrap(), b"hello");
    }
}
