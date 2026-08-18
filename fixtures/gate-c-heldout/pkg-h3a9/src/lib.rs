/// RFC 4648 hex encode (lowercase).
pub fn encode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut out = Vec::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(H[(b >> 4) as usize]);
        out.push(H[(b & 0xf) as usize]);
    }
    Ok(out)
}
