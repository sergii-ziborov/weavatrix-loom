//! Adapter: wvx.reference.base64-decode@1
//!
//! Pure-Rust RFC 4648 standard Base64 decode (multi-impl equality with crate path).

fn val(c: u8) -> Result<u8, String> {
    match c {
        b'A'..=b'Z' => Ok(c - b'A'),
        b'a'..=b'z' => Ok(c - b'a' + 26),
        b'0'..=b'9' => Ok(c - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(format!("invalid base64 char 0x{c:02x}")),
    }
}

/// Decode standard Base64 ASCII to raw bytes (pure).
pub fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if bytes.len() % 4 != 0 {
        return Err(format!(
            "base64 length must be multiple of 4 (got {})",
            bytes.len()
        ));
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = bytes[i + 1];
        let b2 = bytes[i + 2];
        let b3 = bytes[i + 3];
        if b0 == b'=' || b1 == b'=' {
            return Err("invalid padding position".into());
        }
        let n0 = val(b0)? as u32;
        let n1 = val(b1)? as u32;
        let pad2 = b2 == b'=';
        let pad3 = b3 == b'=';
        if pad2 && !pad3 {
            return Err("invalid padding".into());
        }
        let n2 = if pad2 { 0 } else { val(b2)? as u32 };
        let n3 = if pad3 { 0 } else { val(b3)? as u32 };
        let n = (n0 << 18) | (n1 << 12) | (n2 << 6) | n3;
        out.push(((n >> 16) & 0xff) as u8);
        if !pad2 {
            out.push(((n >> 8) & 0xff) as u8);
        }
        if !pad3 {
            out.push((n & 0xff) as u8);
        }
        i += 4;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base64_standard_decode;
    use crate::reference_base64_encode;

    #[test]
    fn equals_crate_and_roundtrip() {
        let samples: &[&[u8]] = &[b"", b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"];
        for s in samples {
            let e = reference_base64_encode::encode(s).unwrap();
            assert_eq!(decode(&e).unwrap(), *s);
            assert_eq!(
                decode(&e).unwrap(),
                base64_standard_decode::decode(&e).unwrap()
            );
        }
    }
}
