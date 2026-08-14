//! Adapter: wvx.reference.base64-encode@1
//!
//! Pure-Rust RFC 4648 standard Base64 encode (multi-impl equality with crate path).

const TABLE: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode raw bytes as standard Base64 ASCII (pure).
pub fn encode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(TABLE[((n >> 18) & 0x3f) as usize]);
        out.push(TABLE[((n >> 12) & 0x3f) as usize]);
        out.push(TABLE[((n >> 6) & 0x3f) as usize]);
        out.push(TABLE[(n & 0x3f) as usize]);
        i += 3;
    }
    let rem = bytes.len() - i;
    if rem == 1 {
        let n = (bytes[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 0x3f) as usize]);
        out.push(TABLE[((n >> 12) & 0x3f) as usize]);
        out.push(b'=');
        out.push(b'=');
    } else if rem == 2 {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 0x3f) as usize]);
        out.push(TABLE[((n >> 12) & 0x3f) as usize]);
        out.push(TABLE[((n >> 6) & 0x3f) as usize]);
        out.push(b'=');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base64_standard_encode;

    #[test]
    fn equals_crate() {
        let samples: &[&[u8]] = &[b"", b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"];
        for s in samples {
            assert_eq!(
                encode(s).unwrap(),
                base64_standard_encode::encode(s).unwrap(),
                "mismatch on {s:?}"
            );
        }
    }
}
