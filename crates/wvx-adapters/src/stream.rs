//! 64 KiB `Read` pump for adapters that can update incrementally.
//!
//! Input is streamed. Output is still collected (`Vec<u8>`). This is not a
//! streaming IR, not async pipes, and not a multi-stage Reader graph.

use std::io::Read;

pub const CHUNK: usize = 64 * 1024;

pub fn pump<R, F>(mut reader: R, mut on_chunk: F) -> Result<(), String>
where
    R: Read,
    F: FnMut(&[u8]) -> Result<(), String>,
{
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("stream-read: {e}"))?;
        if n == 0 {
            return Ok(());
        }
        on_chunk(&buf[..n])?;
    }
}
