//! Adapter: wvx.reference.hex-encode@1
//!
//! Lowercase hex encode (oneshot). Output is ASCII UTF-8 bytes.

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Encode raw bytes as lowercase hex ASCII.
pub fn encode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize]);
        out.push(HEX[(b & 0xf) as usize]);
    }
    Ok(out)
}

pub fn encode_read<R: std::io::Read>(reader: R) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    crate::stream::pump(reader, |chunk| {
        for &b in chunk {
            out.push(HEX[(b >> 4) as usize]);
            out.push(HEX[(b & 0xf) as usize]);
        }
        Ok(())
    })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        assert_eq!(encode(b"").unwrap(), b"");
        assert_eq!(encode(b"abc").unwrap(), b"616263");
        assert_eq!(encode(&[0x00, 0xff]).unwrap(), b"00ff");
        assert_eq!(encode_read(&b"abc"[..]).unwrap(), b"616263");
    }
}
