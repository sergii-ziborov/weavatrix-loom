//! Built-in playground handlers for the JSON pilot pipeline.
//!
//! Multiple **implementations** may fulfill the same capability. Swapping
//! `instance.implementation` does not change bindings or capability ids.

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
    // I/O
    reg.register_default(IoInputBytes);
    reg.register_default(IoOutputBytes);

    // data.json.path_set@1 — reference map-insert + JSON Pointer (Gate A)
    reg.register_default(JsonPathSet);
    reg.register(JsonPointerPathSet);

    // data.json.parse@1 — three independent code paths (Gate A)
    reg.register_default(SerdeJsonParse);
    reg.register(ReferenceJsonParse);
    reg.register(JsonCrateParse);

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
            implementation_id: "json-crate.parse@1",
            capability_key: "data.json.parse@1",
            label: "crates.io json crate parse",
        },
        PilotImplementation {
            implementation_id: "wvx.reference.path-set@1",
            capability_key: "data.json.path_set@1",
            label: "Reference JSON path set",
        },
        PilotImplementation {
            implementation_id: "serde-json.pointer-set@1",
            capability_key: "data.json.path_set@1",
            label: "serde_json JSON Pointer path set",
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
struct JsonCrateParse;

impl ErasedComponent for SerdeJsonParse {
    fn implementation_id(&self) -> &str {
        "serde-json.parse-owned@1"
    }
    fn capability_key(&self) -> &str {
        "data.json.parse@1"
    }
    fn execute(&self, inputs: &WvxValueMap, _config: &ConfigMap) -> Result<WvxValueMap, String> {
        let bytes = require_bytes(inputs, "data.json.parse")?;
        let value = wvx_adapters::serde_json_parse_owned::parse(bytes)?;
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
        let value = wvx_adapters::reference_json_parse::parse(bytes)?;
        let mut out = WvxValueMap::new();
        out.insert("value".into(), WvxValue::Json(value));
        Ok(out)
    }
}

impl ErasedComponent for JsonCrateParse {
    fn implementation_id(&self) -> &str {
        "json-crate.parse@1"
    }
    fn capability_key(&self) -> &str {
        "data.json.parse@1"
    }
    fn execute(&self, inputs: &WvxValueMap, _config: &ConfigMap) -> Result<WvxValueMap, String> {
        let bytes = require_bytes(inputs, "data.json.parse")?;
        let value = wvx_adapters::json_crate_parse::parse(bytes)?;
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
        let bytes = wvx_adapters::serde_json_serialize::serialize(value)?;
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
        let bytes = wvx_adapters::reference_json_serialize::serialize(value)?;
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
        let bytes = wvx_adapters::reference_json_serialize_pretty::serialize(value)?;
        let mut out = WvxValueMap::new();
        out.insert("bytes".into(), WvxValue::Bytes(bytes));
        Ok(out)
    }
}

// --- path set --------------------------------------------------------------

struct JsonPathSet;
struct JsonPointerPathSet;

fn path_set_config(config: &ConfigMap) -> Result<(&str, serde_json::Value), String> {
    let path = config
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "data.json.path_set: config.path (string) is required".to_string())?;
    let set_to = config
        .get("value")
        .cloned()
        .ok_or_else(|| "data.json.path_set: config.value is required".to_string())?;
    Ok((path, set_to))
}

impl ErasedComponent for JsonPathSet {
    fn implementation_id(&self) -> &str {
        "wvx.reference.path-set@1"
    }
    fn capability_key(&self) -> &str {
        "data.json.path_set@1"
    }
    fn execute(&self, inputs: &WvxValueMap, config: &ConfigMap) -> Result<WvxValueMap, String> {
        let value = require_json(inputs, "data.json.path_set")?.clone();
        let (path, set_to) = path_set_config(config)?;
        let value = wvx_adapters::reference_path_set::path_set(value, path, set_to)?;
        let mut out = WvxValueMap::new();
        out.insert("value".into(), WvxValue::Json(value));
        Ok(out)
    }
}

impl ErasedComponent for JsonPointerPathSet {
    fn implementation_id(&self) -> &str {
        "serde-json.pointer-set@1"
    }
    fn capability_key(&self) -> &str {
        "data.json.path_set@1"
    }
    fn execute(&self, inputs: &WvxValueMap, config: &ConfigMap) -> Result<WvxValueMap, String> {
        let value = require_json(inputs, "data.json.path_set")?.clone();
        let (path, set_to) = path_set_config(config)?;
        let value = wvx_adapters::serde_json_pointer_set::path_set(value, path, set_to)?;
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
