//! Adapter: flate2.gunzip@1 — gzip decompress (read_to_end).

use flate2::read::GzDecoder;
use std::io::Read;

/// Decompress gzip `bytes` to the original payload.
pub fn decompress(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut dec = GzDecoder::new(bytes);
    let mut out = Vec::new();
    dec.read_to_end(&mut out)
        .map_err(|e| format!("gunzip: {e}"))?;
    Ok(out)
}
