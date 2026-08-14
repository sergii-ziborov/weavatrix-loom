pub fn transform(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let s = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    Ok(s.to_uppercase().into_bytes())
}
