//! Adapter: wvx.reference.json-serialize@1
use serde_json::Value;

pub fn serialize(value: &Value) -> Result<Vec<u8>, String> {
    let mut out = String::new();
    write_value(&mut out, value, 0, false)?;
    Ok(out.into_bytes())
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
                    for _ in 0..(depth + 1) {
                        out.push_str("  ");
                    }
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
                for _ in 0..depth {
                    out.push_str("  ");
                }
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
                    for _ in 0..(depth + 1) {
                        out.push_str("  ");
                    }
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
                for _ in 0..depth {
                    out.push_str("  ");
                }
            }
            out.push('}');
        }
    }
    Ok(())
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
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
