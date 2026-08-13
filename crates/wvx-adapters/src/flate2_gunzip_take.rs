//! Adapter: flate2.gunzip-take@1 — decompress via `take` + `read_to_end` on unlimited take.

use flate2::read::GzDecoder;
use std::io::{Read, Take};

/// Decompress gzip by wrapping decoder in `Take::new(..., u64::MAX)`.
pub fn decompress(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let dec = GzDecoder::new(bytes);
    let mut limited: Take<GzDecoder<&[u8]>> = dec.take(u64::MAX);
    let mut out = Vec::new();
    limited
        .read_to_end(&mut out)
        .map_err(|e| format!("gunzip-take: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{flate2_gzip, flate2_gunzip};

    #[test]
    fn matches_oneshot() {
        let raw = b"take path";
        let c = flate2_gzip::compress(raw).unwrap();
        assert_eq!(decompress(&c).unwrap(), flate2_gunzip::decompress(&c).unwrap());
    }
}
