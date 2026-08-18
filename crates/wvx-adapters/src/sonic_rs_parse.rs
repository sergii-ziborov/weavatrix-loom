//! Adapter: `sonic-rs.parse@1`
//!
//! SIMD JSON (CloudWeGo sonic). Not the default parse — serde_json stays first.
//! Fastest on x86_64/aarch64 with host SIMD; other targets fall back.
//! Output is `serde_json::Value` via serde.

use serde_json::Value;

pub fn parse(bytes: &[u8]) -> Result<Value, String> {
    std::str::from_utf8(bytes).map_err(|e| format!("invalid-unicode: {e}"))?;
    sonic_rs::from_slice(bytes).map_err(|e| format!("invalid-syntax: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_object_and_number() {
        let v = parse(br#"{"hello":"world","n":-42.5}"#).unwrap();
        assert_eq!(v["hello"], "world");
        assert!(v["n"].is_number());
    }

    #[test]
    fn rejects_trailing_junk() {
        let err = parse(b"{} x").unwrap_err();
        assert!(err.contains("invalid-syntax"), "{err}");
    }

    #[test]
    fn rejects_bad_utf8() {
        let err = parse(&[0xff, 0xfe]).unwrap_err();
        assert!(err.contains("invalid-unicode"), "{err}");
    }
}
