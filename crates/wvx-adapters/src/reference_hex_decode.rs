//! Adapter: wvx.reference.hex-decode@1
//!
//! Decode lowercase/uppercase hex ASCII to raw bytes (strict even length).

fn nibble(c: u8) -> Result<u8, String> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(format!("invalid hex digit 0x{c:02x}")),
    }
}

/// Decode hex ASCII bytes to raw.
pub fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() % 2 != 0 {
        return Err(format!(
            "hex length must be even (got {})",
            bytes.len()
        ));
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = nibble(bytes[i])?;
        let lo = nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference_hex_encode;

    #[test]
    fn roundtrip() {
        let samples: &[&[u8]] = &[b"", b"Hi", b"\x00\xff\x10"];
        for s in samples {
            let enc = reference_hex_encode::encode(s).unwrap();
            assert_eq!(decode(&enc).unwrap(), *s);
        }
    }

    #[test]
    fn mixed_case() {
        assert_eq!(decode(b"AbCd").unwrap(), [0xab, 0xcd]);
    }

    #[test]
    fn rejects_odd() {
        assert!(decode(b"abc").is_err());
    }
}
