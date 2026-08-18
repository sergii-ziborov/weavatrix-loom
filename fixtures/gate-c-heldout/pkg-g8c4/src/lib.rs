use base64::{engine::general_purpose::STANDARD, Engine};
/// RFC 4648 standard Base64 encode.
pub fn encode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    Ok(STANDARD.encode(bytes).into_bytes())
}
