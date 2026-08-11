//! Built-in playground handlers for the JSON pilot pipeline.
//!
//! Multiple **implementations** may fulfill the same capability. Swapping
//! `instance.implementation` does not change bindings or capability ids.

use crate::lite_json;
use crate::{ConfigMap, ErasedComponent, HandlerRegistry, WvxValueMap};
use wvx_types::WvxValue;

/// Catalog entry for discoverability (CLI / MCP).
#[derive(Debug, Clone)]
pub struct PilotImplementation {
    pub implementation_id: &'static str,
    pub capability_key: &'static str,
    pub label: &'static str,
}

/// Register I/O + dual JSON parse/serialize + path-set handlers.
pub fn register_pilot_handlers(reg: &mut HandlerRegistry) {
    // I/O + path-set (single reference impl each)
    reg.register_default(IoInputBytes);
    reg.register_default(IoOutputBytes);
    reg.register_default(JsonPathSet);

    // data.json.parse@1 — two independent code paths
    reg.register_default(SerdeJsonParse);
    reg.register(ReferenceJsonParse);

    // data.json.serialize@1 — serde compact (default) + reference pretty/compact
    reg.register_default(SerdeJsonSerialize);
    reg.register(ReferenceJsonSerializeCompact);
    reg.register(ReferenceJsonSerializePretty);
}

pub fn list_pilot_implementations() -> Vec<PilotImplementation> {
    vec![
        PilotImplementation {
            implementation_id: "wvx.reference.io-input-bytes@1",
            capability_key: "io.input.bytes@1",
            label: "Reference input bytes",
        },
        PilotImplementation {
            implementation_id: "wvx.reference.io-output-bytes@1",
            capability_key: "io.output.bytes@1",
            label: "Reference output bytes",
        },
        PilotImplementation {
            implementation_id: "serde-json.parse-owned@1",
            capability_key: "data.json.parse@1",
            label: "serde_json owned parse",
        },
        PilotImplementation {
            implementation_id: "wvx.reference.json-parse@1",
            capability_key: "data.json.parse@1",
            label: "WVX lite recursive-descent parse",
        },
        PilotImplementation {
            implementation_id: "wvx.reference.path-set@1",
            capability_key: "data.json.path_set@1",
            label: "Reference JSON path set",
        },
        PilotImplementation {
            implementation_id: "serde-json.serialize@1",
            capability_key: "data.json.serialize@1",
            label: "serde_json compact serialize",
        },
        PilotImplementation {
            implementation_id: "wvx.reference.json-serialize@1",
            capability_key: "data.json.serialize@1",
            label: "WVX lite compact serialize",
        },
        PilotImplementation {
            implementation_id: "wvx.reference.json-serialize-pretty@1",
            capability_key: "data.json.serialize@1",
            label: "WVX lite pretty serialize",
        },
    ]
}

// --- I/O -------------------------------------------------------------------

struct IoInputBytes;
struct IoOutputBytes;

impl ErasedComponent for IoInputBytes {
    fn implementation_id(&self) -> &str {
        "wvx.reference.io-input-bytes@1"
    }
    fn capability_key(&self) -> &str {
        "io.input.bytes@1"
    }
    fn execute(&self, inputs: &WvxValueMap, config: &ConfigMap) -> Result<WvxValueMap, String> {
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
        Err("io.input.bytes: no bytes provided (seed entry outputs or config.bytes)".into())
    }
}

impl ErasedComponent for IoOutputBytes {
    fn implementation_id(&self) -> &str {
        "wvx.reference.io-output-bytes@1"
    }
    fn capability_key(&self) -> &str {
        "io.output.bytes@1"
    }
    fn execute(&self, inputs: &WvxValueMap, _config: &ConfigMap) -> Result<WvxValueMap, String> {
        if inputs.get("bytes").is_none() {
            return Err("io.output.bytes: missing input port `bytes`".into());
        }
        Ok(WvxValueMap::new())
    }
}

// --- parse -----------------------------------------------------------------

struct SerdeJsonParse;
struct ReferenceJsonParse;

impl ErasedComponent for SerdeJsonParse {
    fn implementation_id(&self) -> &str {
        "serde-json.parse-owned@1"
    }
    fn capability_key(&self) -> &str {
        "data.json.parse@1"
    }
    fn execute(&self, inputs: &WvxValueMap, _config: &ConfigMap) -> Result<WvxValueMap, String> {
        let bytes = require_bytes(inputs, "data.json.parse")?;
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| format!("invalid-syntax: {e}"))?;
        let mut out = WvxValueMap::new();
        out.insert("value".into(), WvxValue::Json(value));
        Ok(out)
    }
}

impl ErasedComponent for ReferenceJsonParse {
    fn implementation_id(&self) -> &str {
        "wvx.reference.json-parse@1"
    }
    fn capability_key(&self) -> &str {
        "data.json.parse@1"
    }
    fn execute(&self, inputs: &WvxValueMap, _config: &ConfigMap) -> Result<WvxValueMap, String> {
        let bytes = require_bytes(inputs, "data.json.parse")?;
        let value = lite_json::parse_slice(bytes)?;
        let mut out = WvxValueMap::new();
        out.insert("value".into(), WvxValue::Json(value));
        Ok(out)
    }
}

// --- serialize -------------------------------------------------------------

struct SerdeJsonSerialize;
struct ReferenceJsonSerializeCompact;
struct ReferenceJsonSerializePretty;

impl ErasedComponent for SerdeJsonSerialize {
    fn implementation_id(&self) -> &str {
        "serde-json.serialize@1"
    }
    fn capability_key(&self) -> &str {
        "data.json.serialize@1"
    }
    fn execute(&self, inputs: &WvxValueMap, _config: &ConfigMap) -> Result<WvxValueMap, String> {
        let value = require_json(inputs, "data.json.serialize")?;
        let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        let mut out = WvxValueMap::new();
        out.insert("bytes".into(), WvxValue::Bytes(bytes));
        Ok(out)
    }
}

impl ErasedComponent for ReferenceJsonSerializeCompact {
    fn implementation_id(&self) -> &str {
        "wvx.reference.json-serialize@1"
    }
    fn capability_key(&self) -> &str {
        "data.json.serialize@1"
    }
    fn execute(&self, inputs: &WvxValueMap, _config: &ConfigMap) -> Result<WvxValueMap, String> {
        let value = require_json(inputs, "data.json.serialize")?;
        let bytes = lite_json::serialize_compact(value)?;
        let mut out = WvxValueMap::new();
        out.insert("bytes".into(), WvxValue::Bytes(bytes));
        Ok(out)
    }
}

impl ErasedComponent for ReferenceJsonSerializePretty {
    fn implementation_id(&self) -> &str {
        "wvx.reference.json-serialize-pretty@1"
    }
    fn capability_key(&self) -> &str {
        "data.json.serialize@1"
    }
    fn execute(&self, inputs: &WvxValueMap, _config: &ConfigMap) -> Result<WvxValueMap, String> {
        let value = require_json(inputs, "data.json.serialize")?;
        let bytes = lite_json::serialize_pretty(value)?;
        let mut out = WvxValueMap::new();
        out.insert("bytes".into(), WvxValue::Bytes(bytes));
        Ok(out)
    }
}

// --- path set --------------------------------------------------------------

struct JsonPathSet;

impl ErasedComponent for JsonPathSet {
    fn implementation_id(&self) -> &str {
        "wvx.reference.path-set@1"
    }
    fn capability_key(&self) -> &str {
        "data.json.path_set@1"
    }
    fn execute(&self, inputs: &WvxValueMap, config: &ConfigMap) -> Result<WvxValueMap, String> {
        let mut value = require_json(inputs, "data.json.path_set")?.clone();
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

fn require_bytes<'a>(inputs: &'a WvxValueMap, cap: &str) -> Result<&'a [u8], String> {
    match inputs.get("bytes") {
        Some(WvxValue::Bytes(b)) => Ok(b.as_slice()),
        Some(_) => Err(format!("{cap}: port `bytes` must be bytes")),
        None => Err(format!("{cap}: missing port `bytes`")),
    }
}

fn require_json<'a>(inputs: &'a WvxValueMap, cap: &str) -> Result<&'a serde_json::Value, String> {
    match inputs.get("value") {
        Some(WvxValue::Json(v)) => Ok(v),
        Some(_) => Err(format!("{cap}: port `value` must be json.value")),
        None => Err(format!("{cap}: missing port `value`")),
    }
}

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
