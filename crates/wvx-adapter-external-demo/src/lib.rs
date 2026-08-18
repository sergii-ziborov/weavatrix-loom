//! External Gate F adapter: a **semantically equivalent** JSON parser.
//!
//! Deliberately **not** registered in `wvx-runtime` pilot tables or the legacy
//! compiler `match` arms. Host registration uses feature `host` + SDK plugins.
//!
//! Same contract as `serde-json.parse-owned@1` (`data.json.parse@1`):
//! parse RFC 8259 JSON without transforming values. This proves core-independent
//! extensibility without claiming a different capability under the same signature.

use serde_json::Value;

/// Implementation id for this external adapter.
pub const IMPLEMENTATION_ID: &str = "external.demo.json-parse@1";
pub const CAPABILITY_KEY: &str = "data.json.parse@1";

/// Parse JSON bytes into a value. Semantically equivalent to `serde_json::from_slice`
/// after a UTF-8 check (capability error families: `invalid-unicode`, `invalid-syntax`).
pub fn parse(bytes: &[u8]) -> Result<Value, String> {
    std::str::from_utf8(bytes).map_err(|e| format!("invalid-unicode: {e}"))?;
    serde_json::from_slice(bytes).map_err(|e| format!("invalid-syntax: {e}"))
}

/// Register into the process-wide SDK plugin table (host feature only).
#[cfg(feature = "host")]
pub fn register() {
    use std::sync::Once;
    use wvx_component_sdk::{bytes_to_json_handler, register_plugin};
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        register_plugin(IMPLEMENTATION_ID, CAPABILITY_KEY, || {
            bytes_to_json_handler(IMPLEMENTATION_ID, CAPABILITY_KEY, parse)
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_object_without_transform() {
        let v = parse(br#"{"hello":"world"}"#).unwrap();
        assert_eq!(v["hello"], "world");
    }

    #[test]
    fn rejects_bad_utf8() {
        let err = parse(&[0xff]).unwrap_err();
        assert!(err.starts_with("invalid-unicode"), "{err}");
    }
}
