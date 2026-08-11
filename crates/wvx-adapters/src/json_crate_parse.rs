//! Adapter: `json-crate.parse@1`
//!
//! Uses the crates.io `json` package (Maciej Hirsz) — a separate codebase from
//! `serde_json` and the in-tree reference lite parser. Output is normalized to
//! `serde_json::Value` so the rest of Loom can keep a single canonical boundary type.

use serde_json::{Map, Number, Value};

pub fn parse(bytes: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("invalid-unicode: {e}"))?;
    let v = json::parse(text).map_err(|e| format!("invalid-syntax: {e}"))?;
    to_serde(&v)
}

fn to_serde(v: &json::JsonValue) -> Result<Value, String> {
    match v {
        json::JsonValue::Null => Ok(Value::Null),
        json::JsonValue::Boolean(b) => Ok(Value::Bool(*b)),
        json::JsonValue::Number(n) => {
            // Always go through the textual form so decimals (e.g. -42.5) are preserved.
            // as_fixed_point_i64(0) truncates fractions and must not be used alone.
            let s = n.to_string();
            if let Ok(i) = s.parse::<i64>() {
                Ok(Value::Number(Number::from(i)))
            } else if let Ok(u) = s.parse::<u64>() {
                Ok(Value::Number(Number::from(u)))
            } else {
                let f: f64 = s
                    .parse()
                    .map_err(|e| format!("invalid-syntax: number {e}"))?;
                Number::from_f64(f)
                    .map(Value::Number)
                    .ok_or_else(|| "invalid-syntax: non-finite number".to_string())
            }
        }
        json::JsonValue::Short(s) => Ok(Value::String(s.to_string())),
        json::JsonValue::String(s) => Ok(Value::String(s.clone())),
        json::JsonValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(to_serde(item)?);
            }
            Ok(Value::Array(out))
        }
        json::JsonValue::Object(obj) => {
            let mut map = Map::new();
            for (k, val) in obj.iter() {
                map.insert(k.to_string(), to_serde(val)?);
            }
            Ok(Value::Object(map))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_object() {
        let v = parse(br#"{"hello":"world","n":1}"#).unwrap();
        assert_eq!(v["hello"], "world");
        assert_eq!(v["n"], 1);
    }
}
