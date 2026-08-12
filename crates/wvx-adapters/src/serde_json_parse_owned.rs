//! Adapter: serde-json.parse-owned@1
//!
//! Error codes align with `data.json.parse@1` capability:
//! `invalid-unicode` for non-UTF-8 input, `invalid-syntax` otherwise.
use serde_json::Value;

pub fn parse(bytes: &[u8]) -> Result<Value, String> {
    // Classify before serde so bad UTF-8 is not folded into a generic syntax error.
    std::str::from_utf8(bytes).map_err(|e| format!("invalid-unicode: {e}"))?;
    serde_json::from_slice(bytes).map_err(|e| format!("invalid-syntax: {e}"))
}
