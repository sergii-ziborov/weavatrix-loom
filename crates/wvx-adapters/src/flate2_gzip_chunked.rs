//! Adapter: flate2.gzip-chunked@1 — gzip compress via 1 KiB writes (multi-impl).

use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;

const CHUNK: usize = 1024;

/// Gzip-compress with chunked writes (same level as flate2.gzip@1).
pub fn compress(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    for chunk in bytes.chunks(CHUNK) {
        enc.write_all(chunk).map_err(|e| format!("gzip-write: {e}"))?;
    }
    enc.finish().map_err(|e| format!("gzip-finish: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flate2_gunzip;

    #[test]
    fn decompresses() {
        let raw = vec![7u8; 5000];
        let c = compress(&raw).unwrap();
        assert_eq!(flate2_gunzip::decompress(&c).unwrap(), raw);
    }
}
