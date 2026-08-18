//! Adapter: `simd-json.parse@1`
//!
//! SIMD JSON (simdjson port). Input is copied into a mutable buffer because
//! simd-json parses in place. Output is `serde_json::Value` via the serde impl.
//!
//! Error families match `data.json.parse@1`: `invalid-unicode` then `invalid-syntax`.

use serde_json::Value;

pub fn parse(bytes: &[u8]) -> Result<Value, String> {
    std::str::from_utf8(bytes).map_err(|e| format!("invalid-unicode: {e}"))?;
    // simd-json mutates and may need extra padding capacity.
    let mut buf = bytes.to_vec();
    simd_json::serde::from_slice::<Value>(&mut buf).map_err(|e| format!("invalid-syntax: {e}"))
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
