//! Adapter: wvx.reference.text-lowercase@1
//!
//! Unicode-aware UTF-8 lowercasing (`str::to_lowercase`).

/// Lowercase all Unicode scalar values in UTF-8 `bytes`.
pub fn transform(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("invalid-unicode: {e}"))?;
    Ok(text.to_lowercase().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_ascii() {
        assert_eq!(transform(b"HELLO").unwrap(), b"hello");
    }

    #[test]
    fn rejects_invalid_utf8() {
        assert!(transform(&[0xff]).unwrap_err().contains("invalid-unicode"));
    }
}
