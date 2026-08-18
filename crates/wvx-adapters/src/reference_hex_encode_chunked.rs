//! Adapter: wvx.reference.hex-encode-chunked@1
//!
//! Same contract as hex-encode, processed in 64-byte chunks (multi-impl equality).

const HEX: &[u8; 16] = b"0123456789abcdef";
const CHUNK: usize = 64;

/// Encode raw bytes as lowercase hex ASCII (chunked path).
pub fn encode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(bytes.len() * 2);
    for chunk in bytes.chunks(CHUNK) {
        for &b in chunk {
            out.push(HEX[(b >> 4) as usize]);
            out.push(HEX[(b & 0xf) as usize]);
        }
    }
    Ok(out)
}

pub fn encode_read<R: std::io::Read>(reader: R) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    crate::stream::pump(reader, |chunk| {
        for piece in chunk.chunks(CHUNK) {
            for &b in piece {
                out.push(HEX[(b >> 4) as usize]);
                out.push(HEX[(b & 0xf) as usize]);
            }
        }
        Ok(())
    })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference_hex_encode;

    #[test]
    fn equals_oneshot() {
        let samples: &[&[u8]] = &[b"", b"x", b"hello world", &[0u8; 200]];
        for s in samples {
            assert_eq!(encode(s).unwrap(), reference_hex_encode::encode(s).unwrap());
        }
    }
}
