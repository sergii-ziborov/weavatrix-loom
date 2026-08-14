//! External-style Base64 encode package (Gate C Domain 4).
//!
//! Outside Loom product crates — Forge extract/map target.

use base64::{engine::general_purpose::STANDARD, Engine};

pub fn encode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    Ok(STANDARD.encode(bytes).into_bytes())
}
