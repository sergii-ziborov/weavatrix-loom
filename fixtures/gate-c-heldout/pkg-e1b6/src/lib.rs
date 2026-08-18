use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;
/// gzip (RFC 1952) compress.
pub fn compress(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    e.write_all(bytes).map_err(|e| e.to_string())?;
    e.finish().map_err(|e| e.to_string())
}
