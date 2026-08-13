//! Adapter: wvx.reference.text-uppercase@1
//!
//! Unicode-aware UTF-8 uppercasing (`str::to_uppercase`).

/// Uppercase all Unicode scalar values in UTF-8 `bytes`.
pub fn transform(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("invalid-unicode: {e}"))?;
    Ok(text.to_uppercase().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uppercases_ascii_and_unicode() {
        assert_eq!(transform(b"Hello").unwrap(), b"HELLO");
        assert_eq!(transform("straße".as_bytes()).unwrap(), "STRASSE".as_bytes());
    }

    #[test]
    fn rejects_invalid_utf8() {
        let err = transform(&[0xff, 0xfe]).unwrap_err();
        assert!(err.contains("invalid-unicode"));
    }
}
