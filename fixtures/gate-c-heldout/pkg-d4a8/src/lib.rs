/// BLAKE3 keyed-less digest.
pub fn digest(bytes: &[u8]) -> Result<Vec<u8>, String> {
    Ok(blake3::hash(bytes).as_bytes().to_vec())
}
