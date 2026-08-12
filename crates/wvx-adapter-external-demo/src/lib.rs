//! External demo adapter for Gate F.
//!
//! Deliberately **not** registered in `wvx-runtime` pilot tables or the legacy
//! compiler `match` arms. Host registration uses feature `host` + SDK plugins.

use serde_json::Value;

/// Implementation id for this external adapter.
pub const IMPLEMENTATION_ID: &str = "external.demo.upper-parse@1";
pub const CAPABILITY_KEY: &str = "data.json.parse@1";

/// Parse JSON and uppercase all **string leaf values** (demo transform).
pub fn upper_parse(bytes: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("invalid-unicode: {e}"))?;
    let mut v: Value =
        serde_json::from_str(text).map_err(|e| format!("invalid-syntax: {e}"))?;
    uppercase_strings(&mut v);
    Ok(v)
}

fn uppercase_strings(v: &mut Value) {
    match v {
        Value::String(s) => *s = s.to_uppercase(),
        Value::Array(items) => {
            for i in items {
                uppercase_strings(i);
            }
        }
        Value::Object(map) => {
            for (_k, val) in map.iter_mut() {
                uppercase_strings(val);
            }
        }
        _ => {}
    }
}

/// Register into the process-wide SDK plugin table (host feature only).
#[cfg(feature = "host")]
pub fn register() {
    use std::sync::Once;
    use wvx_component_sdk::{bytes_to_json_handler, register_plugin};
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        register_plugin(IMPLEMENTATION_ID, CAPABILITY_KEY, || {
            bytes_to_json_handler(IMPLEMENTATION_ID, CAPABILITY_KEY, upper_parse)
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uppercases_strings() {
        let v = upper_parse(br#"{"hello":"world"}"#).unwrap();
        assert_eq!(v["hello"], "WORLD");
    }
}
