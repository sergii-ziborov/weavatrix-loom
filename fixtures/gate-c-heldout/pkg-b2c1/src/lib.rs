use serde_json::Value;
/// Compact JSON serialize.
pub fn serialize(value: &Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|e| e.to_string())
}
