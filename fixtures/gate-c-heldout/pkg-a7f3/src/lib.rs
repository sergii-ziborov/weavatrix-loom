use serde_json::Value;
/// RFC 8259 JSON parse.
pub fn parse(bytes: &[u8]) -> Result<Value, String> {
    std::str::from_utf8(bytes).map_err(|e| format!("invalid-unicode: {e}"))?;
    serde_json::from_slice(bytes).map_err(|e| format!("invalid-syntax: {e}"))
}
