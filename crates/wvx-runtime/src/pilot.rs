//! Built-in playground handlers for **I/O only**.
//!
//! Transform implementations register via `wvx-component-sdk` plugins
//! (`wvx-adapters::register_pilot_plugins`, external crates). See ADR-0011.

use crate::{ConfigMap, ErasedComponent, HandlerRegistry, WvxValueMap};
use wvx_types::WvxValue;

/// Catalog entry for discoverability (CLI / MCP).
#[derive(Debug, Clone)]
pub struct PilotImplementation {
    pub implementation_id: &'static str,
    pub capability_key: &'static str,
    pub label: &'static str,
}

/// Register pilot I/O handlers only (no transform match table).
pub fn register_pilot_handlers(reg: &mut HandlerRegistry) {
    reg.register_default(IoInputBytes);
    reg.register_default(IoOutputBytes);
}

/// Full catalog of pilot + known SDK transform implementations (documentation).
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
            label: "serde_json owned parse (SDK)",
        },
        PilotImplementation {
            implementation_id: "wvx.reference.json-parse@1",
            capability_key: "data.json.parse@1",
            label: "WVX lite recursive-descent parse (SDK)",
        },
        PilotImplementation {
            implementation_id: "json-crate.parse@1",
            capability_key: "data.json.parse@1",
            label: "crates.io json crate parse (SDK)",
        },
        PilotImplementation {
            implementation_id: "external.demo.upper-parse@1",
            capability_key: "data.json.parse@1",
            label: "Gate F external upper-parse (SDK)",
        },
        PilotImplementation {
            implementation_id: "wvx.reference.path-set@1",
            capability_key: "data.json.path_set@1",
            label: "Reference JSON path set (SDK)",
        },
        PilotImplementation {
            implementation_id: "serde-json.pointer-set@1",
            capability_key: "data.json.path_set@1",
            label: "serde_json JSON Pointer path set (SDK)",
        },
        PilotImplementation {
            implementation_id: "serde-json.serialize@1",
            capability_key: "data.json.serialize@1",
            label: "serde_json compact serialize (SDK)",
        },
        PilotImplementation {
            implementation_id: "wvx.reference.json-serialize@1",
            capability_key: "data.json.serialize@1",
            label: "WVX lite compact serialize (SDK)",
        },
        PilotImplementation {
            implementation_id: "wvx.reference.json-serialize-pretty@1",
            capability_key: "data.json.serialize@1",
            label: "WVX lite pretty serialize (SDK)",
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
