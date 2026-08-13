//! Implementation emit catalog: pilot pure-fn templates + Gate F SDK emit.
//!
//! Runtime handlers are **not** defined here — only static export call shapes.
//! Prefer `Implementation.sdk.emit` from the registry when present.

use std::collections::{BTreeMap, BTreeSet};
use wvx_ir::SdkEmit;

/// Module name inside `wvx_adapters` for vendoring legacy pilot modules.
pub fn crate_module(implementation_id: &str) -> Option<&'static str> {
    match implementation_id {
        "serde-json.parse-owned@1" => Some("serde_json_parse_owned"),
        "wvx.reference.json-parse@1" => Some("reference_json_parse"),
        "json-crate.parse@1" => Some("json_crate_parse"),
        "serde-json.serialize@1" => Some("serde_json_serialize"),
        "wvx.reference.json-serialize@1" => Some("reference_json_serialize"),
        "wvx.reference.json-serialize-pretty@1" => Some("reference_json_serialize_pretty"),
        "wvx.reference.path-set@1" => Some("reference_path_set"),
        "serde-json.pointer-set@1" => Some("serde_json_pointer_set"),
        "wvx.reference.text-uppercase@1" => Some("reference_text_uppercase"),
        "wvx.reference.text-ascii-upper@1" => Some("reference_text_ascii_upper"),
        "wvx.reference.text-lowercase@1" => Some("reference_text_lowercase"),
        "wvx.reference.text-ascii-lower@1" => Some("reference_text_ascii_lower"),
        "wvx.reference.io-input-bytes@1" | "wvx.reference.io-output-bytes@1" => None,
        _ => None,
    }
}

pub fn supports(
    implementation_id: &str,
    capability_key: &str,
    sdk_emit: Option<&SdkEmit>,
) -> bool {
    if is_passthrough_io(capability_key) {
        return true;
    }
    if sdk_emit.is_some() || built_in_sdk_emit(implementation_id).is_some() {
        return true;
    }
    crate_module(implementation_id).is_some()
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
        "data.text.uppercase@1" => Some("wvx.reference.text-uppercase@1"),
        "data.text.lowercase@1" => Some("wvx.reference.text-lowercase@1"),
        _ => None,
    }
}

pub fn known_implementation_ids() -> Vec<&'static str> {
    vec![
        "serde-json.parse-owned@1",
        "wvx.reference.json-parse@1",
        "json-crate.parse@1",
        "external.demo.upper-parse@1",
        "serde-json.serialize@1",
        "wvx.reference.json-serialize@1",
        "wvx.reference.json-serialize-pretty@1",
        "wvx.reference.path-set@1",
        "serde-json.pointer-set@1",
        "wvx.reference.text-uppercase@1",
        "wvx.reference.text-ascii-upper@1",
        "wvx.reference.text-lowercase@1",
        "wvx.reference.text-ascii-lower@1",
        "wvx.reference.io-input-bytes@1",
        "wvx.reference.io-output-bytes@1",
    ]
}

pub fn needs_external_adapters(impls: &BTreeSet<String>) -> bool {
    impls.iter().any(|id| crate_module(id).is_some())
}

/// Built-in SDK emit templates for pilot adapters (no registry required for export).
pub fn built_in_sdk_emit(implementation_id: &str) -> Option<SdkEmit> {
    let (crate_name, crate_path, template) = match implementation_id {
        "serde-json.parse-owned@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::serde_json_parse_owned::parse({bytes}.as_slice())?",
        ),
        "wvx.reference.json-parse@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::reference_json_parse::parse({bytes}.as_slice())?",
        ),
        "json-crate.parse@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::json_crate_parse::parse({bytes}.as_slice())?",
        ),
        "external.demo.upper-parse@1" => (
            "wvx-adapter-external-demo",
            Some("crates/wvx-adapter-external-demo"),
            "wvx_adapter_external_demo::upper_parse({bytes}.as_slice())?",
        ),
        "serde-json.serialize@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::serde_json_serialize::serialize(&{value})?",
        ),
        "wvx.reference.json-serialize@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::reference_json_serialize::serialize(&{value})?",
        ),
        "wvx.reference.json-serialize-pretty@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::reference_json_serialize_pretty::serialize(&{value})?",
        ),
        "wvx.reference.text-uppercase@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::reference_text_uppercase::transform({bytes}.as_slice())?",
        ),
        "wvx.reference.text-ascii-upper@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::reference_text_ascii_upper::transform({bytes}.as_slice())?",
        ),
        "wvx.reference.text-lowercase@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::reference_text_lowercase::transform({bytes}.as_slice())?",
        ),
        "wvx.reference.text-ascii-lower@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::reference_text_ascii_lower::transform({bytes}.as_slice())?",
        ),
        // path_set still needs config inlining — special template with {value} only;
        // config path/value are filled by emit_call_path_set.
        "wvx.reference.path-set@1" | "serde-json.pointer-set@1" => return None,
        _ => return None,
    };
    Some(SdkEmit {
        crate_name: crate_name.into(),
        crate_path: crate_path.map(str::to_string),
        template: template.into(),
    })
}

/// Emit call expression: registry SDK emit → built-in SDK catalog → path_set special.
pub fn emit_call(
    implementation_id: &str,
    input_exprs: &BTreeMapPorts,
    config_json: &serde_json::Value,
    sdk_emit: Option<&SdkEmit>,
) -> Result<String, String> {
    let effective = sdk_emit.cloned().or_else(|| built_in_sdk_emit(implementation_id));
    if let Some(sdk) = effective {
        let mut map = BTreeMap::new();
        for (k, v) in input_exprs {
            map.insert(k.clone(), v.clone());
        }
        return render_sdk_template(&sdk.template, &map);
    }

    // path_set: config-dependent (still data-driven module name, not runtime match).
    if matches!(
        implementation_id,
        "wvx.reference.path-set@1" | "serde-json.pointer-set@1"
    ) {
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
        let module = crate_module(implementation_id).ok_or("path_set module")?;
        return Ok(format!(
            "wvx_adapters::{module}::path_set({value}, {path_lit}, {val_lit})?"
        ));
    }

    Err(format!(
        "no code emitter for implementation `{implementation_id}` (provide Implementation.sdk.emit)"
    ))
}

fn render_sdk_template(
    template: &str,
    input_exprs: &BTreeMap<String, String>,
) -> Result<String, String> {
    let mut out = template.to_string();
    for (port, expr) in input_exprs {
        let needle = format!("{{{port}}}");
        out = out.replace(&needle, expr);
    }
    if out.contains('{') && out.contains('}') {
        return Err(format!(
            "sdk emit template has unresolved placeholders: {out}"
        ));
    }
    Ok(out)
}

type BTreeMapPorts = std::collections::BTreeMap<String, String>;
