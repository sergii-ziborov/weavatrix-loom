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
        "simd-json.parse@1" => Some("simd_json_parse"),
        "serde-json.serialize@1" => Some("serde_json_serialize"),
        "wvx.reference.json-serialize@1" => Some("reference_json_serialize"),
        "wvx.reference.json-serialize-pretty@1" => Some("reference_json_serialize_pretty"),
        "wvx.reference.path-set@1" => Some("reference_path_set"),
        "serde-json.pointer-set@1" => Some("serde_json_pointer_set"),
        "wvx.reference.text-uppercase@1" => Some("reference_text_uppercase"),
        "wvx.reference.text-ascii-upper@1" => Some("reference_text_ascii_upper"),
        "wvx.reference.text-lowercase@1" => Some("reference_text_lowercase"),
        "wvx.reference.text-ascii-lower@1" => Some("reference_text_ascii_lower"),
        "sha2.sha256@1" => Some("sha2_sha256"),
        "sha2.sha256-streaming@1" => Some("sha2_sha256_streaming"),
        "sha2.sha256-chunked@1" => Some("sha2_sha256_chunked"),
        "sha2.sha256-update-all@1" => Some("sha2_sha256_update_all"),
        "blake3.blake3@1" => Some("blake3_hash"),
        "blake3.blake3-parallel@1" => Some("blake3_hash_parallel"),
        "flate2.gzip@1" => Some("flate2_gzip"),
        "flate2.gzip-chunked@1" => Some("flate2_gzip_chunked"),
        "flate2.gzip-oneshot@1" => Some("flate2_gzip_oneshot"),
        "flate2.gunzip@1" => Some("flate2_gunzip"),
        "flate2.gunzip-chunked@1" => Some("flate2_gunzip_chunked"),
        "flate2.gunzip-take@1" => Some("flate2_gunzip_take"),
        "wvx.reference.hex-encode@1" => Some("reference_hex_encode"),
        "wvx.reference.hex-encode-chunked@1" => Some("reference_hex_encode_chunked"),
        "wvx.reference.hex-decode@1" => Some("reference_hex_decode"),
        "wvx.reference.hex-decode-table@1" => Some("reference_hex_decode_table"),
        "base64.standard-encode@1" => Some("base64_standard_encode"),
        "wvx.reference.base64-encode@1" => Some("reference_base64_encode"),
        "base64.standard-decode@1" => Some("base64_standard_decode"),
        "wvx.reference.base64-decode@1" => Some("reference_base64_decode"),
        "wvx.reference.io-input-bytes@1" | "wvx.reference.io-output-bytes@1" => None,
        _ => None,
    }
}

pub fn supports(implementation_id: &str, capability_key: &str, sdk_emit: Option<&SdkEmit>) -> bool {
    if is_passthrough_io(capability_key) {
        return true;
    }
    if sdk_emit.is_some() || built_in_sdk_emit(implementation_id).is_some() {
        return true;
    }
    crate_module(implementation_id).is_some()
}

pub fn is_passthrough_io(capability_key: &str) -> bool {
    matches!(capability_key, "io.input.bytes@1" | "io.output.bytes@1")
}

pub fn default_implementation(capability_key: &str) -> Option<&'static str> {
    match capability_key {
        "io.input.bytes@1" => Some("wvx.reference.io-input-bytes@1"),
        "io.output.bytes@1" => Some("wvx.reference.io-output-bytes@1"),
        "data.json.parse@1" => Some("serde-json.parse-owned@1"),
        "data.json.serialize@1" => Some("serde-json.serialize@1"),
        "data.json.path_set@1" => Some("wvx.reference.path-set@1"),
        "data.text.unicode_uppercase@1" | "data.text.uppercase@1" => {
            Some("wvx.reference.text-uppercase@1")
        }
        "data.text.unicode_lowercase@1" | "data.text.lowercase@1" => {
            Some("wvx.reference.text-lowercase@1")
        }
        "data.text.ascii_uppercase@1" => Some("wvx.reference.text-ascii-upper@1"),
        "data.text.ascii_lowercase@1" => Some("wvx.reference.text-ascii-lower@1"),
        "data.hash.sha256@1" => Some("sha2.sha256@1"),
        "data.hash.blake3@1" => Some("blake3.blake3@1"),
        "data.compress.gzip@1" => Some("flate2.gzip@1"),
        "data.compress.gunzip@1" => Some("flate2.gunzip@1"),
        "data.codec.hex_encode@1" => Some("wvx.reference.hex-encode@1"),
        "data.codec.hex_decode@1" => Some("wvx.reference.hex-decode@1"),
        "data.codec.base64_encode@1" => Some("base64.standard-encode@1"),
        "data.codec.base64_decode@1" => Some("base64.standard-decode@1"),
        _ => None,
    }
}

pub fn known_implementation_ids() -> Vec<&'static str> {
    vec![
        "serde-json.parse-owned@1",
        "wvx.reference.json-parse@1",
        "json-crate.parse@1",
        "simd-json.parse@1",
        "external.demo.json-parse@1",
        "serde-json.serialize@1",
        "wvx.reference.json-serialize@1",
        "wvx.reference.json-serialize-pretty@1",
        "wvx.reference.path-set@1",
        "serde-json.pointer-set@1",
        "wvx.reference.text-uppercase@1",
        "wvx.reference.text-ascii-upper@1",
        "wvx.reference.text-lowercase@1",
        "wvx.reference.text-ascii-lower@1",
        "sha2.sha256@1",
        "sha2.sha256-streaming@1",
        "sha2.sha256-chunked@1",
        "sha2.sha256-update-all@1",
        "blake3.blake3@1",
        "blake3.blake3-parallel@1",
        "flate2.gzip@1",
        "flate2.gzip-chunked@1",
        "flate2.gzip-oneshot@1",
        "flate2.gunzip@1",
        "flate2.gunzip-chunked@1",
        "flate2.gunzip-take@1",
        "wvx.reference.hex-encode@1",
        "wvx.reference.hex-encode-chunked@1",
        "wvx.reference.hex-decode@1",
        "wvx.reference.hex-decode-table@1",
        "base64.standard-encode@1",
        "wvx.reference.base64-encode@1",
        "base64.standard-decode@1",
        "wvx.reference.base64-decode@1",
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
        "simd-json.parse@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::simd_json_parse::parse({bytes}.as_slice())?",
        ),
        "external.demo.json-parse@1" => (
            "wvx-adapter-external-demo",
            Some("crates/wvx-adapter-external-demo"),
            "wvx_adapter_external_demo::parse({bytes}.as_slice())?",
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
        "sha2.sha256@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::sha2_sha256::digest({bytes}.as_slice())?",
        ),
        "sha2.sha256-streaming@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::sha2_sha256_streaming::digest({bytes}.as_slice())?",
        ),
        "blake3.blake3@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::blake3_hash::digest({bytes}.as_slice())?",
        ),
        "blake3.blake3-parallel@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::blake3_hash_parallel::digest({bytes}.as_slice())?",
        ),
        "sha2.sha256-chunked@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::sha2_sha256_chunked::digest({bytes}.as_slice())?",
        ),
        "sha2.sha256-update-all@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::sha2_sha256_update_all::digest({bytes}.as_slice())?",
        ),
        "flate2.gzip@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::flate2_gzip::compress({bytes}.as_slice())?",
        ),
        "flate2.gzip-chunked@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::flate2_gzip_chunked::compress({bytes}.as_slice())?",
        ),
        "flate2.gzip-oneshot@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::flate2_gzip_oneshot::compress({bytes}.as_slice())?",
        ),
        "flate2.gunzip@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::flate2_gunzip::decompress({bytes}.as_slice())?",
        ),
        "flate2.gunzip-chunked@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::flate2_gunzip_chunked::decompress({bytes}.as_slice())?",
        ),
        "flate2.gunzip-take@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::flate2_gunzip_take::decompress({bytes}.as_slice())?",
        ),
        "wvx.reference.hex-encode@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::reference_hex_encode::encode({bytes}.as_slice())?",
        ),
        "wvx.reference.hex-encode-chunked@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::reference_hex_encode_chunked::encode({bytes}.as_slice())?",
        ),
        "wvx.reference.hex-decode@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::reference_hex_decode::decode({bytes}.as_slice())?",
        ),
        "wvx.reference.hex-decode-table@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::reference_hex_decode_table::decode({bytes}.as_slice())?",
        ),
        "base64.standard-encode@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::base64_standard_encode::encode({bytes}.as_slice())?",
        ),
        "wvx.reference.base64-encode@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::reference_base64_encode::encode({bytes}.as_slice())?",
        ),
        "base64.standard-decode@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::base64_standard_decode::decode({bytes}.as_slice())?",
        ),
        "wvx.reference.base64-decode@1" => (
            "wvx-adapters",
            Some("crates/wvx-adapters"),
            "wvx_adapters::reference_base64_decode::decode({bytes}.as_slice())?",
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
    let effective = sdk_emit
        .cloned()
        .or_else(|| built_in_sdk_emit(implementation_id));
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
