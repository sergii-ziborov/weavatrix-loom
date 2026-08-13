//! Adapter: flate2.gzip-oneshot@1 — gzip via `read::GzEncoder` oneshot read.

use flate2::read::GzEncoder;
use flate2::Compression;
use std::io::Read;

/// Gzip-compress by reading from a `GzEncoder` wrapper around the input.
pub fn compress(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut enc = GzEncoder::new(bytes, Compression::default());
    let mut out = Vec::new();
    enc.read_to_end(&mut out)
        .map_err(|e| format!("gzip-read: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flate2_gunzip;

    #[test]
    fn decompresses() {
        let raw = b"oneshot gzip path";
        let c = compress(raw).unwrap();
        assert_eq!(flate2_gunzip::decompress(&c).unwrap(), raw);
    }
}
