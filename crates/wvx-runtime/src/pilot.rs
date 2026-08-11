//! Built-in playground handlers for the JSON pilot pipeline.
//!
//! These are reference components for development, not production adapter crates.

use crate::{ConfigMap, ErasedComponent, HandlerRegistry, WvxValueMap};
use wvx_types::WvxValue;

/// Register I/O + JSON parse / path-set / serialize handlers used by the pilot fixture.
pub fn register_pilot_handlers(reg: &mut HandlerRegistry) {
    reg.register(IoInputBytes);
    reg.register(IoOutputBytes);
    reg.register(JsonParse);
    reg.register(JsonPathSet);
    reg.register(JsonSerialize);
}

struct IoInputBytes;
struct IoOutputBytes;
struct JsonParse;
struct JsonPathSet;
struct JsonSerialize;

impl ErasedComponent for IoInputBytes {
    fn capability_key(&self) -> &str {
        "io.input.bytes@1"
    }

    fn execute(&self, inputs: &WvxValueMap, config: &ConfigMap) -> Result<WvxValueMap, String> {
        // Prefer bound/seeded input; else config.bytes as base64 or utf-8 string; else empty.
        if let Some(v) = inputs.get("bytes") {
            let mut out = WvxValueMap::new();
            out.insert("bytes".into(), v.clone());
            return Ok(out);
        }
        if let Some(cfg) = config.get("bytes") {
            let bytes = json_to_bytes(cfg)?;
            let mut out = WvxValueMap::new();
            out.insert("bytes".into(), WvxValue::Bytes(bytes));
            return Ok(out);
        }
        // When seeded via entry_outputs the runtime skips missing handlers;
        // if we are invoked without data, fail closed.
        Err("io.input.bytes: no bytes provided (seed entry outputs or config.bytes)".into())
    }
}

impl ErasedComponent for IoOutputBytes {
    fn capability_key(&self) -> &str {
        "io.output.bytes@1"
    }

    fn execute(&self, inputs: &WvxValueMap, _config: &ConfigMap) -> Result<WvxValueMap, String> {
        // Sink: accept bytes, emit nothing (value retained on input binding path).
        if inputs.get("bytes").is_none() {
            return Err("io.output.bytes: missing input port `bytes`".into());
        }
        Ok(WvxValueMap::new())
    }
}

impl ErasedComponent for JsonParse {
    fn capability_key(&self) -> &str {
        "data.json.parse@1"
    }

    fn execute(&self, inputs: &WvxValueMap, _config: &ConfigMap) -> Result<WvxValueMap, String> {
        let bytes = match inputs.get("bytes") {
            Some(WvxValue::Bytes(b)) => b.as_slice(),
            Some(_) => return Err("data.json.parse: port `bytes` must be bytes".into()),
            None => return Err("data.json.parse: missing port `bytes`".into()),
        };
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| format!("invalid-syntax: {e}"))?;
        let mut out = WvxValueMap::new();
        out.insert("value".into(), WvxValue::Json(value));
        Ok(out)
    }
}

impl ErasedComponent for JsonSerialize {
    fn capability_key(&self) -> &str {
        "data.json.serialize@1"
    }

    fn execute(&self, inputs: &WvxValueMap, _config: &ConfigMap) -> Result<WvxValueMap, String> {
        let value = match inputs.get("value") {
            Some(WvxValue::Json(v)) => v,
            Some(_) => return Err("data.json.serialize: port `value` must be json.value".into()),
            None => return Err("data.json.serialize: missing port `value`".into()),
        };
        let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        let mut out = WvxValueMap::new();
        out.insert("bytes".into(), WvxValue::Bytes(bytes));
        Ok(out)
    }
}

impl ErasedComponent for JsonPathSet {
    fn capability_key(&self) -> &str {
        "data.json.path_set@1"
    }

    fn execute(&self, inputs: &WvxValueMap, config: &ConfigMap) -> Result<WvxValueMap, String> {
        let mut value = match inputs.get("value") {
            Some(WvxValue::Json(v)) => v.clone(),
            Some(_) => return Err("data.json.path_set: port `value` must be json.value".into()),
            None => return Err("data.json.path_set: missing port `value`".into()),
        };

        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "data.json.path_set: config.path (string) is required".to_string())?;
        let set_to = config
            .get("value")
            .cloned()
            .ok_or_else(|| "data.json.path_set: config.value is required".to_string())?;

        set_json_path(&mut value, path, set_to)?;

        let mut out = WvxValueMap::new();
        out.insert("value".into(), WvxValue::Json(value));
        Ok(out)
    }
}

/// Minimal JSON Pointer-ish setter for v0.1: `/key` or `key` on object roots only.
fn set_json_path(
    root: &mut serde_json::Value,
    path: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    let key = path.trim().trim_start_matches('/');
    if key.is_empty() || key.contains('/') {
        return Err(format!(
            "data.json.path_set: only single-segment paths supported in v0.1 (got `{path}`)"
        ));
    }
    match root {
        serde_json::Value::Object(map) => {
            map.insert(key.to_string(), value);
            Ok(())
        }
        _ => Err("data.json.path_set: root value must be a JSON object".into()),
    }
}

fn json_to_bytes(v: &serde_json::Value) -> Result<Vec<u8>, String> {
    if let Some(s) = v.as_str() {
        return Ok(s.as_bytes().to_vec());
    }
    if let Some(arr) = v.as_array() {
        let mut bytes = Vec::with_capacity(arr.len());
        for item in arr {
            let n = item
                .as_u64()
                .ok_or_else(|| "config.bytes array entries must be u8".to_string())?;
            if n > 255 {
                return Err("config.bytes array entry out of u8 range".into());
            }
            bytes.push(n as u8);
        }
        return Ok(bytes);
    }
    Err("config.bytes must be a string or array of bytes".into())
}
