//! Adapter: `zlib-rs.gzip@1` — gzip compress via zlib-rs (not miniz_oxide).

use zlib_rs::deflate::{compress_slice, DeflateConfig};
use zlib_rs::ReturnCode;

/// Gzip-compress `bytes` (zlib-rs, gzip wrapper, default level).
pub fn compress(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let bound = bytes.len() + bytes.len() / 8 + 128;
    let mut out = vec![0u8; bound.max(256)];
    let config = DeflateConfig {
        window_bits: 16 + 15,
        ..DeflateConfig::default()
    };
    let (written, code) = compress_slice(&mut out, bytes, config);
    match code {
        ReturnCode::Ok | ReturnCode::StreamEnd => Ok(written.to_vec()),
        other => Err(format!("gzip: {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_flate2_and_self() {
        let raw = b"Weavatrix Loom zlib-rs gzip";
        let c = compress(raw).unwrap();
        assert_eq!(crate::flate2_gunzip::decompress(&c).unwrap(), raw);
        assert_eq!(crate::zlib_rs_gunzip::decompress(&c).unwrap(), raw);
    }
}
