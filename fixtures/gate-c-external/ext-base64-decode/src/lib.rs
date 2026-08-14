use base64::{engine::general_purpose::STANDARD, Engine};

pub fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    STANDARD.decode(bytes).map_err(|e| e.to_string())
}
