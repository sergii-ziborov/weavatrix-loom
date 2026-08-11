//! Maps implementation ids to the external `wvx-adapters` crate.

use std::collections::BTreeSet;

/// Module name inside `wvx_adapters` for this implementation.
pub fn crate_module(implementation_id: &str) -> Option<&'static str> {
    match implementation_id {
        "serde-json.parse-owned@1" => Some("serde_json_parse_owned"),
        "wvx.reference.json-parse@1" => Some("reference_json_parse"),
        "json-crate.parse@1" => Some("json_crate_parse"),
        "serde-json.serialize@1" => Some("serde_json_serialize"),
        "wvx.reference.json-serialize@1" => Some("reference_json_serialize"),
        "wvx.reference.json-serialize-pretty@1" => Some("reference_json_serialize_pretty"),
        "wvx.reference.path-set@1" => Some("reference_path_set"),
        "wvx.reference.io-input-bytes@1" | "wvx.reference.io-output-bytes@1" => None,
        _ => None,
    }
}

pub fn supports(implementation_id: &str, capability_key: &str) -> bool {
    if is_passthrough_io(capability_key) {
        return true;
    }
    match (implementation_id, capability_key) {
        ("serde-json.parse-owned@1", "data.json.parse@1") => true,
        ("wvx.reference.json-parse@1", "data.json.parse@1") => true,
        ("json-crate.parse@1", "data.json.parse@1") => true,
        ("serde-json.serialize@1", "data.json.serialize@1") => true,
        ("wvx.reference.json-serialize@1", "data.json.serialize@1") => true,
        ("wvx.reference.json-serialize-pretty@1", "data.json.serialize@1") => true,
        ("wvx.reference.path-set@1", "data.json.path_set@1") => true,
        _ => crate_module(implementation_id).is_some(),
    }
}

pub fn is_passthrough_io(capability_key: &str) -> bool {
    matches!(
        capability_key,
        "io.input.bytes@1" | "io.output.bytes@1"
    )
}

pub fn default_implementation(capability_key: &str) -> Option<&'static str> {
    match capability_key {
        "io.input.bytes@1" => Some("wvx.reference.io-input-bytes@1"),
        "io.output.bytes@1" => Some("wvx.reference.io-output-bytes@1"),
        "data.json.parse@1" => Some("serde-json.parse-owned@1"),
        "data.json.serialize@1" => Some("serde-json.serialize@1"),
        "data.json.path_set@1" => Some("wvx.reference.path-set@1"),
        _ => None,
    }
}

pub fn known_implementation_ids() -> Vec<&'static str> {
    vec![
        "serde-json.parse-owned@1",
        "wvx.reference.json-parse@1",
        "json-crate.parse@1",
        "serde-json.serialize@1",
        "wvx.reference.json-serialize@1",
        "wvx.reference.json-serialize-pretty@1",
        "wvx.reference.path-set@1",
        "wvx.reference.io-input-bytes@1",
        "wvx.reference.io-output-bytes@1",
    ]
}

pub fn needs_external_adapters(impls: &BTreeSet<String>) -> bool {
    impls.iter().any(|id| crate_module(id).is_some())
}

/// Emit call expression using `wvx_adapters::<module>::…`.
pub fn emit_call(
    implementation_id: &str,
    input_exprs: &BTreeMapPorts,
    config_json: &serde_json::Value,
) -> Result<String, String> {
    match implementation_id {
        "serde-json.parse-owned@1" => {
            let bytes = input_exprs.get("bytes").ok_or("parse requires bytes")?;
            Ok(format!(
                "wvx_adapters::serde_json_parse_owned::parse({bytes}.as_slice())?"
            ))
        }
        "wvx.reference.json-parse@1" => {
            let bytes = input_exprs.get("bytes").ok_or("parse requires bytes")?;
            Ok(format!(
                "wvx_adapters::reference_json_parse::parse({bytes}.as_slice())?"
            ))
        }
        "json-crate.parse@1" => {
            let bytes = input_exprs.get("bytes").ok_or("parse requires bytes")?;
            Ok(format!(
                "wvx_adapters::json_crate_parse::parse({bytes}.as_slice())?"
            ))
        }
        "serde-json.serialize@1" => {
            let value = input_exprs.get("value").ok_or("serialize requires value")?;
            Ok(format!(
                "wvx_adapters::serde_json_serialize::serialize(&{value})?"
            ))
        }
        "wvx.reference.json-serialize@1" => {
            let value = input_exprs.get("value").ok_or("serialize requires value")?;
            Ok(format!(
                "wvx_adapters::reference_json_serialize::serialize(&{value})?"
            ))
        }
        "wvx.reference.json-serialize-pretty@1" => {
            let value = input_exprs.get("value").ok_or("serialize requires value")?;
            Ok(format!(
                "wvx_adapters::reference_json_serialize_pretty::serialize(&{value})?"
            ))
        }
        "wvx.reference.path-set@1" => {
            let value = input_exprs.get("value").ok_or("path_set requires value")?;
            let path = config_json
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("path_set config.path required")?;
            let set_val = config_json
                .get("value")
                .ok_or("path_set config.value required")?;
            let path_lit = format!("{:?}", path);
            let val_lit = format!(
                "serde_json::from_str::<serde_json::Value>({}).map_err(|e| e.to_string())?",
                format!("{:?}", set_val.to_string())
            );
            Ok(format!(
                "wvx_adapters::reference_path_set::path_set({value}, {path_lit}, {val_lit})?"
            ))
        }
        other => Err(format!("no code emitter for implementation `{other}`")),
    }
}

type BTreeMapPorts = std::collections::BTreeMap<String, String>;
