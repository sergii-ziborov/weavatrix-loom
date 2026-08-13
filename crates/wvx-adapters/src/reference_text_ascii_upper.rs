//! Adapter: wvx.reference.text-ascii-upper@1
//!
//! ASCII-only uppercasing (non-ASCII bytes left unchanged). Alternate impl for
//! multi-impl swap demos against Unicode uppercase.

/// Uppercase A–Z only; leave other bytes unchanged (no UTF-8 validation).
pub fn transform(bytes: &[u8]) -> Result<Vec<u8>, String> {
    Ok(bytes
        .iter()
        .map(|b| if (b'a'..=b'z').contains(b) { b - 32 } else { *b })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_only() {
        assert_eq!(transform(b"Hello").unwrap(), b"HELLO");
        // non-ascii byte unchanged
        assert_eq!(transform(&[0xc3, 0xa4, b'a']).unwrap(), vec![0xc3, 0xa4, b'A']);
    }
}
