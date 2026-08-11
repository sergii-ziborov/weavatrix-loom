//! Adapter: serde-json.serialize@1
use serde_json::Value;

pub fn serialize(value: &Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|e| e.to_string())
}
