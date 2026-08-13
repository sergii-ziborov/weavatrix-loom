use serde_json::Value;
/// Public API for Forge extract → data.json.parse
pub fn parse(bytes: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}
