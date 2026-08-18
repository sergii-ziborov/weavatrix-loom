//! Adapter: flate2.gzip@1 — gzip compress (default level, write_all).

use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;

/// Gzip-compress `bytes` (flate2 default compression level).
pub fn compress(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(bytes)
        .map_err(|e| format!("gzip-write: {e}"))?;
    enc.finish().map_err(|e| format!("gzip-finish: {e}"))
}

/// Gzip from a `Read` (64 KiB writes). Output is still collected.
pub fn compress_read<R: std::io::Read>(reader: R) -> Result<Vec<u8>, String> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    crate::stream::pump(reader, |chunk| {
        enc.write_all(chunk).map_err(|e| format!("gzip-write: {e}"))
    })?;
    enc.finish().map_err(|e| format!("gzip-finish: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flate2_gunzip;

    #[test]
    fn roundtrip() {
        let raw = b"Weavatrix Loom Domain 3 compression";
        let c = compress(raw).unwrap();
        assert!(c.len() < raw.len() + 64);
        assert_eq!(flate2_gunzip::decompress(&c).unwrap(), raw);
        let streamed = compress_read(&raw[..]).unwrap();
        assert_eq!(flate2_gunzip::decompress(&streamed).unwrap(), raw);
    }
}
