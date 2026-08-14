//! Adapter: wvx.reference.hex-decode-table@1
//!
//! Same contract as hex-decode; uses a 256-entry lookup table (multi-impl equality).

const fn build_table() -> [i8; 256] {
    let mut t = [-1i8; 256];
    let mut i = 0u8;
    while i < 10 {
        t[(b'0' + i) as usize] = i as i8;
        i += 1;
    }
    i = 0;
    while i < 6 {
        t[(b'a' + i) as usize] = (10 + i) as i8;
        t[(b'A' + i) as usize] = (10 + i) as i8;
        i += 1;
    }
    t
}

static TABLE: [i8; 256] = build_table();

/// Decode hex ASCII via lookup table.
pub fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() % 2 != 0 {
        return Err(format!(
            "hex length must be even (got {})",
            bytes.len()
        ));
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = TABLE[bytes[i] as usize];
        let lo = TABLE[bytes[i + 1] as usize];
        if hi < 0 {
            return Err(format!("invalid hex digit 0x{:02x}", bytes[i]));
        }
        if lo < 0 {
            return Err(format!("invalid hex digit 0x{:02x}", bytes[i + 1]));
        }
        out.push(((hi as u8) << 4) | (lo as u8));
        i += 2;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference_hex_decode;

    #[test]
    fn equals_pair_path() {
        let samples: &[&[u8]] = &[b"", b"00ff", b"DeadBeef", b"616263"];
        for s in samples {
            assert_eq!(
                decode(s).unwrap(),
                reference_hex_decode::decode(s).unwrap()
            );
        }
    }
}
