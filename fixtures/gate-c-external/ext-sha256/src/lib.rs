use sha2::{Digest, Sha256};
pub fn digest(bytes: &[u8]) -> Result<Vec<u8>, String> {
    Ok(Sha256::digest(bytes).to_vec())
}
