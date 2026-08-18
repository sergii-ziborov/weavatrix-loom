//! Adapter: `zlib-rs.gunzip@1` — gzip decompress via zlib-rs (not miniz_oxide).

use zlib_rs::inflate::{uncompress_slice, InflateConfig};
use zlib_rs::ReturnCode;

/// Decompress gzip `bytes` with zlib-rs.
pub fn decompress(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let config = InflateConfig {
        window_bits: 16 + 15,
    };
    let mut cap = (bytes.len().saturating_mul(4)).max(1024);
    for _ in 0..8 {
        let mut buf = vec![0u8; cap];
        let (written, code) = uncompress_slice(&mut buf, bytes, config);
        match code {
            ReturnCode::Ok | ReturnCode::StreamEnd => return Ok(written.to_vec()),
            ReturnCode::BufError => {
                cap = cap.saturating_mul(2).max(cap + 4096);
            }
            other => return Err(format!("gunzip: {other:?}")),
        }
    }
    Err("gunzip: output buffer grew too large".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gunzips_flate2_payload() {
        let raw = b"Weavatrix Loom zlib-rs gunzip";
        let c = crate::flate2_gzip::compress(raw).unwrap();
        assert_eq!(decompress(&c).unwrap(), raw);
    }
}
