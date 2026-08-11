//! Adapter: serde-json.parse-owned@1
use serde_json::Value;

pub fn parse(bytes: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("invalid-syntax: {e}"))
}
