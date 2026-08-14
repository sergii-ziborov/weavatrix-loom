//! Adapter: base64.standard-encode@1
//!
//! RFC 4648 standard Base64 encode via the `base64` crate.

use base64::{engine::general_purpose::STANDARD, Engine};

/// Encode raw bytes as standard Base64 ASCII.
pub fn encode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    Ok(STANDARD.encode(bytes).into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector() {
        // "Man" → "TWFu"
        assert_eq!(encode(b"Man").unwrap(), b"TWFu");
        assert_eq!(encode(b"").unwrap(), b"");
    }
}
