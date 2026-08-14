//! Adapter: wvx.reference.json-parse@1
//! Independent recursive-descent JSON parser (pilot alternate implementation).

use serde_json::{Map, Number, Value};

pub fn parse(input: &[u8]) -> Result<Value, String> {
    parse_slice(input)
}

fn parse_slice(input: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(input).map_err(|e| format!("invalid-unicode: {e}"))?;
    let mut p = Parser {
        bytes: text.as_bytes(),
        i: 0,
    };
    let value = p.parse_value()?;
    p.skip_ws();
    if p.i != p.bytes.len() {
        return Err(format!("invalid-syntax: trailing input at byte {}", p.i));
    }
    Ok(value)
}

pub fn serialize_compact(value: &Value) -> Result<Vec<u8>, String> {
    let mut out = String::new();
    write_value(&mut out, value, 0, false)?;
    Ok(out.into_bytes())
}

pub fn serialize_pretty(value: &Value) -> Result<Vec<u8>, String> {
    let mut out = String::new();
    write_value(&mut out, value, 0, true)?;
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out.into_bytes())
}

struct Parser<'a> {
    bytes: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.i).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.i += 1;
        Some(b)
    }

    fn expect(&mut self, want: u8) -> Result<(), String> {
        match self.bump() {
            Some(b) if b == want => Ok(()),
            Some(b) => Err(format!(
                "invalid-syntax: expected '{}' got '{}' at {}",
                want as char, b as char, self.i
            )),
            None => Err("invalid-syntax: unexpected end of input".into()),
        }
    }

    fn parse_value(&mut self) -> Result<Value, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => self.parse_literal(b"null", Value::Null),
            Some(b't') => self.parse_literal(b"true", Value::Bool(true)),
            Some(b'f') => self.parse_literal(b"false", Value::Bool(false)),
            Some(b'"') => Ok(Value::String(self.parse_string()?)),
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            Some(b) => Err(format!(
                "invalid-syntax: unexpected byte {} at {}",
                b, self.i
            )),
            None => Err("invalid-syntax: unexpected end of input".into()),
        }
    }

    fn parse_literal(&mut self, lit: &[u8], value: Value) -> Result<Value, String> {
        for &b in lit {
            self.expect(b)?;
        }
        Ok(value)
    }

    fn parse_object(&mut self) -> Result<Value, String> {
        self.expect(b'{')?;
        self.skip_ws();
        let mut map = Map::new();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Value::Object(map));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let val = self.parse_value()?;
            map.insert(key, val);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                    continue;
                }
                Some(b'}') => {
                    self.i += 1;
                    break;
                }
                Some(b) => {
                    return Err(format!(
                        "invalid-syntax: expected ',' or '}}' got '{}' at {}",
                        b as char, self.i
                    ))
                }
                None => return Err("invalid-syntax: unclosed object".into()),
            }
        }
        Ok(Value::Object(map))
    }

    fn parse_array(&mut self) -> Result<Value, String> {
        self.expect(b'[')?;
        self.skip_ws();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                    continue;
                }
                Some(b']') => {
                    self.i += 1;
                    break;
                }
                Some(b) => {
                    return Err(format!(
                        "invalid-syntax: expected ',' or ']' got '{}' at {}",
                        b as char, self.i
                    ))
                }
                None => return Err("invalid-syntax: unclosed array".into()),
            }
        }
        Ok(Value::Array(items))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            match self.bump() {
                Some(b'"') => return Ok(out),
                Some(b'\\') => match self.bump() {
                    Some(b'"') => out.push('"'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'/') => out.push('/'),
                    Some(b'b') => out.push('\u{0008}'),
                    Some(b'f') => out.push('\u{000c}'),
                    Some(b'n') => out.push('\n'),
                    Some(b'r') => out.push('\r'),
                    Some(b't') => out.push('\t'),
                    Some(b'u') => {
                        return Err(
                            "invalid-syntax: \\u escapes not supported in lite parser v0.1".into(),
                        );
                    }
                    Some(b) => return Err(format!("invalid-syntax: bad escape {}", b as char)),
                    None => return Err("invalid-syntax: truncated escape".into()),
                },
                Some(b) if b < 0x20 => return Err("invalid-syntax: control char in string".into()),
                Some(b) if b < 0x80 => out.push(b as char),
                Some(first) => {
                    // Multi-byte UTF-8 (JSON strings are Unicode; do not cast each byte to char).
                    let start = self.i - 1;
                    let width = utf8_char_width(first).ok_or_else(|| {
                        format!("invalid-unicode: bad UTF-8 lead byte {first:#x} at {start}")
                    })?;
                    if start + width > self.bytes.len() {
                        return Err("invalid-unicode: truncated UTF-8 in string".into());
                    }
                    let s = std::str::from_utf8(&self.bytes[start..start + width])
                        .map_err(|e| format!("invalid-unicode: {e}"))?;
                    out.push_str(s);
                    self.i = start + width;
                }
                None => return Err("invalid-syntax: unclosed string".into()),
            }
        }
    }

    fn parse_number(&mut self) -> Result<Value, String> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        if self.peek() == Some(b'0') {
            self.i += 1;
        } else if matches!(self.peek(), Some(b'1'..=b'9')) {
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        } else {
            return Err(format!("invalid-syntax: bad number at {start}"));
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err("invalid-syntax: bad fraction".into());
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err("invalid-syntax: bad exponent".into());
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        let slice = std::str::from_utf8(&self.bytes[start..self.i]).map_err(|e| e.to_string())?;
        if let Ok(i) = slice.parse::<i64>() {
            return Ok(Value::Number(Number::from(i)));
        }
        let f: f64 = slice
            .parse()
            .map_err(|e| format!("invalid-syntax: number {e}"))?;
        Number::from_f64(f)
            .map(Value::Number)
            .ok_or_else(|| "invalid-syntax: non-finite number".into())
    }
}

fn utf8_char_width(first: u8) -> Option<usize> {
    match first {
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

fn write_value(out: &mut String, value: &Value, depth: usize, pretty: bool) -> Result<(), String> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            out.push('[');
            if pretty && !items.is_empty() {
                out.push('\n');
            }
            for (i, item) in items.iter().enumerate() {
                if pretty {
                    indent(out, depth + 1);
                }
                write_value(out, item, depth + 1, pretty)?;
                if i + 1 != items.len() {
                    out.push(',');
                }
                if pretty {
                    out.push('\n');
                }
            }
            if pretty && !items.is_empty() {
                indent(out, depth);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            if pretty && !map.is_empty() {
                out.push('\n');
            }
            let len = map.len();
            for (i, (k, v)) in map.iter().enumerate() {
                if pretty {
                    indent(out, depth + 1);
                }
                write_string(out, k);
                out.push(':');
                if pretty {
                    out.push(' ');
                }
                write_value(out, v, depth + 1, pretty)?;
                if i + 1 != len {
                    out.push(',');
                }
                if pretty {
                    out.push('\n');
                }
            }
            if pretty && !map.is_empty() {
                indent(out, depth);
            }
            out.push('}');
        }
    }
    Ok(())
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_object() {
        let v = parse_slice(br#"{"hello":"world","n":1}"#).unwrap();
        let bytes = serialize_compact(&v).unwrap();
        let v2 = parse_slice(&bytes).unwrap();
        assert_eq!(v, v2);
    }
}
