//! Adapter: base64.standard-decode@1
//!
//! RFC 4648 standard Base64 decode via the `base64` crate.

use base64::{engine::general_purpose::STANDARD, Engine};

/// Decode standard Base64 ASCII to raw bytes.
pub fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    STANDARD
        .decode(bytes)
        .map_err(|e| format!("base64 decode: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base64_standard_encode;

    #[test]
    fn roundtrip() {
        let samples: &[&[u8]] = &[b"", b"x", b"hello", &[0, 1, 2, 255]];
        for s in samples {
            let e = base64_standard_encode::encode(s).unwrap();
            assert_eq!(decode(&e).unwrap(), *s);
        }
    }
}
