pub fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() % 2 != 0 {
        return Err("invalid-hex: odd length".into());
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = match bytes[i] {
            b'0'..=b'9' => bytes[i] - b'0',
            b'a'..=b'f' => bytes[i] - b'a' + 10,
            b'A'..=b'F' => bytes[i] - b'A' + 10,
            _ => return Err("invalid-hex: bad digit".into()),
        };
        let lo = match bytes[i + 1] {
            b'0'..=b'9' => bytes[i + 1] - b'0',
            b'a'..=b'f' => bytes[i + 1] - b'a' + 10,
            b'A'..=b'F' => bytes[i + 1] - b'A' + 10,
            _ => return Err("invalid-hex: bad digit".into()),
        };
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}
