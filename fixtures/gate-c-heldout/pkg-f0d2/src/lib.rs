use flate2::read::GzDecoder;
use std::io::Read;
/// gzip (RFC 1952) decompress.
pub fn decompress(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut d = GzDecoder::new(bytes);
    let mut out = Vec::new();
    d.read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
