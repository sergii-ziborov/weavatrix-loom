//! Adapter: flate2.gunzip-chunked@1 — decompress via fixed-size reads.

use flate2::read::GzDecoder;
use std::io::Read;

/// Decompress gzip reading into a 512-byte buffer repeatedly.
pub fn decompress(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut dec = GzDecoder::new(bytes);
    let mut out = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = dec
            .read(&mut buf)
            .map_err(|e| format!("gunzip-chunk: {e}"))?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
    }
    Ok(out)
}

pub fn decompress_read<R: Read>(reader: R) -> Result<Vec<u8>, String> {
    let mut dec = GzDecoder::new(reader);
    let mut out = Vec::new();
    let mut buf = [0u8; 512];
    loop {
        let n = dec
            .read(&mut buf)
            .map_err(|e| format!("gunzip-chunk: {e}"))?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{flate2_gunzip, flate2_gzip};

    #[test]
    fn matches_oneshot_gunzip() {
        let raw = b"chunked gunzip equality";
        let c = flate2_gzip::compress(raw).unwrap();
        assert_eq!(
            decompress(&c).unwrap(),
            flate2_gunzip::decompress(&c).unwrap()
        );
    }
}
