//! Pilot conformance suite and dynamic≡static golden checks.
//!
//! Not a full evidence/admission pipeline — focused on Transform MVP:
//! every registered pilot implementation of a capability must agree on
//! shared vectors (semantic equality for JSON), reject shared negative
//! inputs with the same error *code family*, and playground output must
//! match the exported static pipeline on the pilot fixture.
//!
//! Error codes for `data.json.parse@1` (capability contract):
//! `invalid-syntax`, `invalid-unicode`, `depth-limit` (depth not enforced in pilot).
//!
//! Gate E pilot microbench: [`bench`].
//! Profile-driven runner: [`profile_runner`].
//! Multi-domain golden: [`domain_golden`].

pub mod bench;
pub mod domain_golden;
pub mod profile_runner;

pub use bench::{run_pilot_bench, BenchProvenance, BenchReport};
pub use domain_golden::{golden_fixture_bytes, run_domain_goldens, run_full_dynamic_static_matrix};
pub use profile_runner::{
    implementations_for_profile, run_multi_domain_profiles, run_profile_conformance,
    run_profile_doc, run_profile_for_implementation, ProfileRunReport,
};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use wvx_compiler_rust::export_to_directory;
use wvx_ir::Project;
use wvx_runtime::{apply_implementation_overrides, run_project, HandlerRegistry, WvxValueMap};
use wvx_types::WvxValue;

#[derive(Debug, Error)]
pub enum ConformanceError {
    #[error("{0}")]
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceCaseResult {
    pub capability: String,
    pub implementation: String,
    pub case: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceReport {
    pub ok: bool,
    pub cases: Vec<ConformanceCaseResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenReport {
    pub ok: bool,
    pub dynamic_json: serde_json::Value,
    pub static_json: serde_json::Value,
    pub parse_impl: String,
    pub serialize_impl: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Shared vectors: every parse impl must produce the same JSON value.
fn parse_vectors() -> Vec<(&'static str, Vec<u8>, serde_json::Value)> {
    vec![
        (
            "object_simple",
            br#"{"hello":"world"}"#.to_vec(),
            serde_json::json!({"hello": "world"}),
        ),
        (
            "nested",
            br#"{"a":{"b":[1,2,3],"c":true}}"#.to_vec(),
            serde_json::json!({"a":{"b":[1,2,3],"c":true}}),
        ),
        (
            "array",
            br#"[1,"x",null]"#.to_vec(),
            serde_json::json!([1, "x", null]),
        ),
        ("number", br#"-42.5"#.to_vec(), serde_json::json!(-42.5)),
        ("string", br#""loom""#.to_vec(), serde_json::json!("loom")),
        ("bool", br#"false"#.to_vec(), serde_json::json!(false)),
        ("null", br#"null"#.to_vec(), serde_json::json!(null)),
        ("empty_object", br#"{}"#.to_vec(), serde_json::json!({})),
        ("empty_array", br#"[]"#.to_vec(), serde_json::json!([])),
        (
            "unicode",
            // UTF-8 body: {"msg":"привет","emoji":"🧩"}
            "{\"msg\":\"привет\",\"emoji\":\"🧩\"}".as_bytes().to_vec(),
            serde_json::json!({"msg": "привет", "emoji": "🧩"}),
        ),
        (
            "deep_object",
            br#"{"l1":{"l2":{"l3":{"n":1}}}}"#.to_vec(),
            serde_json::json!({"l1":{"l2":{"l3":{"n":1}}}}),
        ),
    ]
}

fn path_set_impls() -> &'static [&'static str] {
    &["wvx.reference.path-set@1", "serde-json.pointer-set@1"]
}

/// Shared path_set cases: (name, input, path, set_value, expected)
fn path_set_vectors() -> Vec<(
    &'static str,
    serde_json::Value,
    &'static str,
    serde_json::Value,
    serde_json::Value,
)> {
    vec![
        (
            "set_tag",
            serde_json::json!({"hello": "world"}),
            "/tag",
            serde_json::json!("loom"),
            serde_json::json!({"hello": "world", "tag": "loom"}),
        ),
        (
            "overwrite_key",
            serde_json::json!({"tag": "old", "n": 1}),
            "/tag",
            serde_json::json!("new"),
            serde_json::json!({"tag": "new", "n": 1}),
        ),
        (
            "set_number",
            serde_json::json!({"a": true}),
            "/count",
            serde_json::json!(42),
            serde_json::json!({"a": true, "count": 42}),
        ),
        (
            "set_object",
            serde_json::json!({}),
            "/meta",
            serde_json::json!({"ok": true}),
            serde_json::json!({"meta": {"ok": true}}),
        ),
        (
            "path_without_slash",
            serde_json::json!({"x": 1}),
            "y",
            serde_json::json!(2),
            serde_json::json!({"x": 1, "y": 2}),
        ),
    ]
}

fn parse_impls() -> &'static [&'static str] {
    &[
        "serde-json.parse-owned@1",
        "wvx.reference.json-parse@1",
        "json-crate.parse@1",
        "simd-json.parse@1",
    ]
}

fn serialize_impls_compact() -> &'static [&'static str] {
    &["serde-json.serialize@1", "wvx.reference.json-serialize@1"]
}

/// Extract the capability error code family from an adapter error string.
///
/// Adapters should emit `{code}: {detail}` using codes from the capability
/// contract (`invalid-syntax`, `invalid-unicode`, …).
pub fn error_code_family(err: &str) -> &str {
    err.split_once(':')
        .map(|(code, _)| code.trim())
        .unwrap_or(err.trim())
}

/// Negative parse vectors: every impl must **fail** with the expected code family.
/// Messages may differ; only the prefix before `:` is contract.
fn parse_negative_vectors() -> Vec<(&'static str, Vec<u8>, &'static str)> {
    vec![
        ("neg_empty", b"".to_vec(), "invalid-syntax"),
        ("neg_whitespace_only", b"   \t\n".to_vec(), "invalid-syntax"),
        ("neg_truncated_object", b"{".to_vec(), "invalid-syntax"),
        ("neg_truncated_array", b"[1,".to_vec(), "invalid-syntax"),
        ("neg_unclosed_string", b"\"hi".to_vec(), "invalid-syntax"),
        ("neg_trailing_value", b"{}{}".to_vec(), "invalid-syntax"),
        (
            "neg_trailing_after_true",
            b"true false".to_vec(),
            "invalid-syntax",
        ),
        ("neg_bad_token", b"undefined".to_vec(), "invalid-syntax"),
        (
            "neg_trailing_comma",
            br#"{"a":1,}"#.to_vec(),
            "invalid-syntax",
        ),
        (
            "neg_single_quotes",
            br#"{'a':1}"#.to_vec(),
            "invalid-syntax",
        ),
        ("neg_plus_number", b"+1".to_vec(), "invalid-syntax"),
        ("neg_bare_key", br#"{a:1}"#.to_vec(), "invalid-syntax"),
        (
            "neg_control_in_string",
            b"\"\x01\"".to_vec(),
            "invalid-syntax",
        ),
        // Whole buffer is not valid UTF-8.
        ("neg_invalid_utf8", vec![0xff, 0xfe], "invalid-unicode"),
        // Valid structure framing but invalid UTF-8 inside a string.
        (
            "neg_invalid_utf8_in_string",
            vec![b'"', 0xff, b'"'],
            "invalid-unicode",
        ),
    ]
}

/// path_set negatives: reject with any error (pilot uses free-form messages).
fn path_set_negative_vectors() -> Vec<(
    &'static str,
    serde_json::Value,
    &'static str,
    serde_json::Value,
)> {
    vec![
        (
            "neg_nested_path",
            serde_json::json!({"a": {"b": 1}}),
            "/a/b",
            serde_json::json!(2),
        ),
        (
            "neg_empty_path",
            serde_json::json!({}),
            "",
            serde_json::json!(1),
        ),
        (
            "neg_root_array",
            serde_json::json!([1, 2]),
            "/0",
            serde_json::json!(9),
        ),
        (
            "neg_root_string",
            serde_json::json!("hi"),
            "/x",
            serde_json::json!(1),
        ),
    ]
}

/// Full playground registry: I/O + SDK pilot transform plugins.
fn pilot_sdk_registry() -> HandlerRegistry {
    wvx_adapters::register_pilot_plugins();
    wvx_component_sdk::registry_with_pilot_and_plugins()
}

/// Run capability-level conformance for pilot JSON handlers.
pub fn run_pilot_conformance() -> ConformanceReport {
    let reg = pilot_sdk_registry();
    let mut cases = Vec::new();

    // --- parse (positive) ---
    for impl_id in parse_impls() {
        for (name, bytes, expected) in parse_vectors() {
            let result = conform_parse(&reg, impl_id, name, bytes.as_slice(), &expected);
            cases.push(result);
        }
    }

    // --- parse (negative: must fail with expected error code family) ---
    for impl_id in parse_impls() {
        for (name, bytes, expected_code) in parse_negative_vectors() {
            cases.push(conform_parse_negative(
                &reg,
                impl_id,
                name,
                bytes.as_slice(),
                expected_code,
            ));
        }
    }

    // --- serialize (compact): re-parse bytes must equal original JSON ---
    let sample = serde_json::json!({"hello":"world","n":1,"ok":true});
    for impl_id in serialize_impls_compact() {
        cases.push(conform_serialize_roundtrip(
            &reg,
            impl_id,
            "roundtrip_object",
            &sample,
        ));
    }

    // pretty serialize: semantic only
    cases.push(conform_serialize_roundtrip(
        &reg,
        "wvx.reference.json-serialize-pretty@1",
        "roundtrip_pretty",
        &sample,
    ));

    // --- path_set (every impl × shared vectors) ---
    for impl_id in path_set_impls() {
        for (name, input, path, set_value, expected) in path_set_vectors() {
            cases.push(conform_path_set(
                &reg, impl_id, name, input, path, set_value, expected,
            ));
        }
    }

    // --- path_set (negative: must fail) ---
    for impl_id in path_set_impls() {
        for (name, input, path, set_value) in path_set_negative_vectors() {
            cases.push(conform_path_set_negative(
                &reg, impl_id, name, input, path, set_value,
            ));
        }
    }

    // --- multi-domain multi-impl equality (competitors must agree bit-for-bit) ---
    cases.extend(run_multi_impl_equality(&reg));

    // --- profile-driven multi-domain suites (when registry-dev is present) ---
    let registry_root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry-dev");
    if registry_root.is_dir() {
        let profile_report = profile_runner::run_multi_domain_profiles(&registry_root, &reg);
        cases.extend(profile_report.cases);
    }

    let ok = cases.iter().all(|c| c.ok);
    ConformanceReport { ok, cases }
}

/// Cross-check competing implementations of the same capability on shared vectors.
///
/// - Hash / hex / base64 encode: bit-equal outputs  
/// - Gunzip: bit-equal after decompressing a fixed gzip blob  
/// - Hex / base64 decode: bit-equal raw recovery
fn run_multi_impl_equality(reg: &HandlerRegistry) -> Vec<ConformanceCaseResult> {
    let mut cases = Vec::new();
    let long = vec![b'x'; 257];
    let vectors: Vec<(&str, &[u8])> = vec![
        ("empty", b""),
        ("ascii", b"hello loom"),
        ("binary", &[0x00, 0xff, 0x10, 0x7f]),
        ("unicode_utf8", "привет🧩".as_bytes()),
        ("long", long.as_slice()),
    ];

    // Domain 2 — SHA-256 multi-impl
    let sha_impls = [
        "sha2.sha256@1",
        "sha2.sha256-streaming@1",
        "sha2.sha256-chunked@1",
        "sha2.sha256-update-all@1",
    ];
    for (vname, input) in &vectors {
        cases.push(conform_multi_bytes(
            reg,
            "data.hash.sha256@1",
            &sha_impls,
            "digest",
            vname,
            input,
            |b| Ok(b.to_vec()),
        ));
    }

    // Domain 3 — gunzip multi-impl (gzip compressed once, all decompress equal)
    if let Ok(gz_handler) = reg.resolve("data.compress.gzip@1", Some("flate2.gzip@1")) {
        let mut inputs = WvxValueMap::new();
        inputs.insert(
            "bytes".into(),
            WvxValue::Bytes(b"Weavatrix multi-impl gunzip equality payload".to_vec()),
        );
        if let Ok(out) = gz_handler.execute(&inputs, &BTreeMap::new()) {
            if let Some(WvxValue::Bytes(gz)) = out.get("bytes") {
                let gunzip_impls = [
                    "flate2.gunzip@1",
                    "flate2.gunzip-chunked@1",
                    "flate2.gunzip-take@1",
                ];
                cases.push(conform_multi_bytes(
                    reg,
                    "data.compress.gunzip@1",
                    &gunzip_impls,
                    "bytes",
                    "gunzip_fixed_blob",
                    gz.as_slice(),
                    |b| Ok(b.to_vec()),
                ));
            }
        }
    }

    // Domain 4 — hex encode multi-impl
    let hex_enc = [
        "wvx.reference.hex-encode@1",
        "wvx.reference.hex-encode-chunked@1",
    ];
    for (vname, input) in &vectors {
        cases.push(conform_multi_bytes(
            reg,
            "data.codec.hex_encode@1",
            &hex_enc,
            "bytes",
            vname,
            input,
            |b| Ok(b.to_vec()),
        ));
    }
    // hex decode multi-impl on encoded form of each vector
    let hex_dec = [
        "wvx.reference.hex-decode@1",
        "wvx.reference.hex-decode-table@1",
    ];
    if let Ok(enc) = reg.resolve(
        "data.codec.hex_encode@1",
        Some("wvx.reference.hex-encode@1"),
    ) {
        for (vname, input) in &vectors {
            let mut inputs = WvxValueMap::new();
            inputs.insert("bytes".into(), WvxValue::Bytes(input.to_vec()));
            if let Ok(out) = enc.execute(&inputs, &BTreeMap::new()) {
                if let Some(WvxValue::Bytes(hex)) = out.get("bytes") {
                    cases.push(conform_multi_bytes(
                        reg,
                        "data.codec.hex_decode@1",
                        &hex_dec,
                        "bytes",
                        vname,
                        hex.as_slice(),
                        |b| Ok(b.to_vec()),
                    ));
                }
            }
        }
    }

    // Domain 4 — base64 encode/decode multi-impl (crate vs pure)
    let b64_enc = ["base64.standard-encode@1", "wvx.reference.base64-encode@1"];
    let b64_dec = ["base64.standard-decode@1", "wvx.reference.base64-decode@1"];
    for (vname, input) in &vectors {
        cases.push(conform_multi_bytes(
            reg,
            "data.codec.base64_encode@1",
            &b64_enc,
            "bytes",
            vname,
            input,
            |b| Ok(b.to_vec()),
        ));
    }
    if let Ok(enc) = reg.resolve(
        "data.codec.base64_encode@1",
        Some("base64.standard-encode@1"),
    ) {
        for (vname, input) in &vectors {
            let mut inputs = WvxValueMap::new();
            inputs.insert("bytes".into(), WvxValue::Bytes(input.to_vec()));
            if let Ok(out) = enc.execute(&inputs, &BTreeMap::new()) {
                if let Some(WvxValue::Bytes(b64)) = out.get("bytes") {
                    cases.push(conform_multi_bytes(
                        reg,
                        "data.codec.base64_decode@1",
                        &b64_dec,
                        "bytes",
                        vname,
                        b64.as_slice(),
                        |b| Ok(b.to_vec()),
                    ));
                }
            }
        }
    }

    cases
}

/// Run `impls` on the same input; all must produce identical bytes on `out_port`.
fn conform_multi_bytes(
    reg: &HandlerRegistry,
    cap: &str,
    impls: &[&str],
    out_port: &str,
    case: &str,
    input: &[u8],
    _identity: fn(&[u8]) -> Result<Vec<u8>, String>,
) -> ConformanceCaseResult {
    let mut outputs: Vec<(String, Vec<u8>)> = Vec::new();
    for impl_id in impls {
        let handler = match reg.resolve(cap, Some(impl_id)) {
            Ok(h) => h,
            Err(e) => {
                return ConformanceCaseResult {
                    capability: cap.into(),
                    implementation: impl_id.to_string(),
                    case: format!("multi_eq:{case}"),
                    ok: false,
                    detail: Some(e.to_string()),
                };
            }
        };
        let mut inputs = WvxValueMap::new();
        inputs.insert("bytes".into(), WvxValue::Bytes(input.to_vec()));
        match handler.execute(&inputs, &BTreeMap::new()) {
            Ok(out) => match out.get(out_port) {
                Some(WvxValue::Bytes(b)) => outputs.push(((*impl_id).into(), b.clone())),
                other => {
                    return ConformanceCaseResult {
                        capability: cap.into(),
                        implementation: (*impl_id).into(),
                        case: format!("multi_eq:{case}"),
                        ok: false,
                        detail: Some(format!("bad output port `{out_port}`: {other:?}")),
                    };
                }
            },
            Err(e) => {
                return ConformanceCaseResult {
                    capability: cap.into(),
                    implementation: (*impl_id).into(),
                    case: format!("multi_eq:{case}"),
                    ok: false,
                    detail: Some(e),
                };
            }
        }
    }
    if outputs.is_empty() {
        return ConformanceCaseResult {
            capability: cap.into(),
            implementation: "(none)".into(),
            case: format!("multi_eq:{case}"),
            ok: false,
            detail: Some("no implementations produced output".into()),
        };
    }
    let reference = &outputs[0].1;
    for (id, bytes) in &outputs[1..] {
        if bytes != reference {
            return ConformanceCaseResult {
                capability: cap.into(),
                implementation: format!("{} vs {}", outputs[0].0, id),
                case: format!("multi_eq:{case}"),
                ok: false,
                detail: Some(format!(
                    "bit mismatch: {} len={} vs {} len={}",
                    outputs[0].0,
                    reference.len(),
                    id,
                    bytes.len()
                )),
            };
        }
    }
    ConformanceCaseResult {
        capability: cap.into(),
        implementation: impls.join(" ≡ "),
        case: format!("multi_eq:{case}"),
        ok: true,
        detail: Some(format!(
            "{} impls agree ({} bytes out)",
            outputs.len(),
            reference.len()
        )),
    }
}

fn conform_parse(
    reg: &HandlerRegistry,
    impl_id: &str,
    case: &str,
    bytes: &[u8],
    expected: &serde_json::Value,
) -> ConformanceCaseResult {
    let cap = "data.json.parse@1";
    let handler = match reg.resolve(cap, Some(impl_id)) {
        Ok(h) => h,
        Err(e) => {
            return ConformanceCaseResult {
                capability: cap.into(),
                implementation: impl_id.into(),
                case: case.into(),
                ok: false,
                detail: Some(e.to_string()),
            };
        }
    };
    let mut inputs = WvxValueMap::new();
    inputs.insert("bytes".into(), WvxValue::Bytes(bytes.to_vec()));
    match handler.execute(&inputs, &BTreeMap::new()) {
        Ok(out) => match out.get("value") {
            Some(WvxValue::Json(v)) if v == expected => ConformanceCaseResult {
                capability: cap.into(),
                implementation: impl_id.into(),
                case: case.into(),
                ok: true,
                detail: None,
            },
            Some(WvxValue::Json(v)) => ConformanceCaseResult {
                capability: cap.into(),
                implementation: impl_id.into(),
                case: case.into(),
                ok: false,
                detail: Some(format!("value mismatch: got {v}, expected {expected}")),
            },
            other => ConformanceCaseResult {
                capability: cap.into(),
                implementation: impl_id.into(),
                case: case.into(),
                ok: false,
                detail: Some(format!("bad output: {other:?}")),
            },
        },
        Err(e) => ConformanceCaseResult {
            capability: cap.into(),
            implementation: impl_id.into(),
            case: case.into(),
            ok: false,
            detail: Some(e),
        },
    }
}

/// Negative parse: must return Err whose code family matches `expected_code`.
fn conform_parse_negative(
    reg: &HandlerRegistry,
    impl_id: &str,
    case: &str,
    bytes: &[u8],
    expected_code: &str,
) -> ConformanceCaseResult {
    let cap = "data.json.parse@1";
    let handler = match reg.resolve(cap, Some(impl_id)) {
        Ok(h) => h,
        Err(e) => {
            return ConformanceCaseResult {
                capability: cap.into(),
                implementation: impl_id.into(),
                case: case.into(),
                ok: false,
                detail: Some(e.to_string()),
            };
        }
    };
    let mut inputs = WvxValueMap::new();
    inputs.insert("bytes".into(), WvxValue::Bytes(bytes.to_vec()));
    match handler.execute(&inputs, &BTreeMap::new()) {
        Ok(out) => ConformanceCaseResult {
            capability: cap.into(),
            implementation: impl_id.into(),
            case: case.into(),
            ok: false,
            detail: Some(format!(
                "expected error `{expected_code}`, but parse succeeded: {out:?}"
            )),
        },
        Err(e) => {
            let got = error_code_family(&e);
            if got == expected_code {
                ConformanceCaseResult {
                    capability: cap.into(),
                    implementation: impl_id.into(),
                    case: case.into(),
                    ok: true,
                    detail: Some(e), // keep message for diagnostics / CLI
                }
            } else {
                ConformanceCaseResult {
                    capability: cap.into(),
                    implementation: impl_id.into(),
                    case: case.into(),
                    ok: false,
                    detail: Some(format!(
                        "expected error code `{expected_code}`, got `{got}` (full: {e})"
                    )),
                }
            }
        }
    }
}

fn conform_serialize_roundtrip(
    reg: &HandlerRegistry,
    impl_id: &str,
    case: &str,
    value: &serde_json::Value,
) -> ConformanceCaseResult {
    let cap = "data.json.serialize@1";
    let handler = match reg.resolve(cap, Some(impl_id)) {
        Ok(h) => h,
        Err(e) => {
            return ConformanceCaseResult {
                capability: cap.into(),
                implementation: impl_id.into(),
                case: case.into(),
                ok: false,
                detail: Some(e.to_string()),
            };
        }
    };
    let mut inputs = WvxValueMap::new();
    inputs.insert("value".into(), WvxValue::Json(value.clone()));
    match handler.execute(&inputs, &BTreeMap::new()) {
        Ok(out) => match out.get("bytes") {
            Some(WvxValue::Bytes(b)) => match serde_json::from_slice::<serde_json::Value>(b) {
                Ok(v) if &v == value => ConformanceCaseResult {
                    capability: cap.into(),
                    implementation: impl_id.into(),
                    case: case.into(),
                    ok: true,
                    detail: None,
                },
                Ok(v) => ConformanceCaseResult {
                    capability: cap.into(),
                    implementation: impl_id.into(),
                    case: case.into(),
                    ok: false,
                    detail: Some(format!("roundtrip mismatch: got {v}")),
                },
                Err(e) => ConformanceCaseResult {
                    capability: cap.into(),
                    implementation: impl_id.into(),
                    case: case.into(),
                    ok: false,
                    detail: Some(format!("invalid json bytes: {e}")),
                },
            },
            other => ConformanceCaseResult {
                capability: cap.into(),
                implementation: impl_id.into(),
                case: case.into(),
                ok: false,
                detail: Some(format!("bad output: {other:?}")),
            },
        },
        Err(e) => ConformanceCaseResult {
            capability: cap.into(),
            implementation: impl_id.into(),
            case: case.into(),
            ok: false,
            detail: Some(e),
        },
    }
}

fn conform_path_set(
    reg: &HandlerRegistry,
    impl_id: &str,
    case: &str,
    input: serde_json::Value,
    path: &str,
    set_value: serde_json::Value,
    expected: serde_json::Value,
) -> ConformanceCaseResult {
    let cap = "data.json.path_set@1";
    let handler = match reg.resolve(cap, Some(impl_id)) {
        Ok(h) => h,
        Err(e) => {
            return ConformanceCaseResult {
                capability: cap.into(),
                implementation: impl_id.into(),
                case: case.into(),
                ok: false,
                detail: Some(e.to_string()),
            };
        }
    };
    let mut inputs = WvxValueMap::new();
    inputs.insert("value".into(), WvxValue::Json(input));
    let mut config = BTreeMap::new();
    config.insert("path".into(), serde_json::Value::String(path.into()));
    config.insert("value".into(), set_value);
    match handler.execute(&inputs, &config) {
        Ok(out) => match out.get("value") {
            Some(WvxValue::Json(v)) if v == &expected => ConformanceCaseResult {
                capability: cap.into(),
                implementation: impl_id.into(),
                case: case.into(),
                ok: true,
                detail: None,
            },
            Some(WvxValue::Json(v)) => ConformanceCaseResult {
                capability: cap.into(),
                implementation: impl_id.into(),
                case: case.into(),
                ok: false,
                detail: Some(format!("got {v}, expected {expected}")),
            },
            other => ConformanceCaseResult {
                capability: cap.into(),
                implementation: impl_id.into(),
                case: case.into(),
                ok: false,
                detail: Some(format!("bad output: {other:?}")),
            },
        },
        Err(e) => ConformanceCaseResult {
            capability: cap.into(),
            implementation: impl_id.into(),
            case: case.into(),
            ok: false,
            detail: Some(e),
        },
    }
}

/// Negative path_set: must fail (any error). Pilot does not yet standardize codes here.
fn conform_path_set_negative(
    reg: &HandlerRegistry,
    impl_id: &str,
    case: &str,
    input: serde_json::Value,
    path: &str,
    set_value: serde_json::Value,
) -> ConformanceCaseResult {
    let cap = "data.json.path_set@1";
    let handler = match reg.resolve(cap, Some(impl_id)) {
        Ok(h) => h,
        Err(e) => {
            return ConformanceCaseResult {
                capability: cap.into(),
                implementation: impl_id.into(),
                case: case.into(),
                ok: false,
                detail: Some(e.to_string()),
            };
        }
    };
    let mut inputs = WvxValueMap::new();
    inputs.insert("value".into(), WvxValue::Json(input));
    let mut config = BTreeMap::new();
    config.insert("path".into(), serde_json::Value::String(path.into()));
    config.insert("value".into(), set_value);
    match handler.execute(&inputs, &config) {
        Ok(out) => ConformanceCaseResult {
            capability: cap.into(),
            implementation: impl_id.into(),
            case: case.into(),
            ok: false,
            detail: Some(format!("expected error, but path_set succeeded: {out:?}")),
        },
        Err(e) => ConformanceCaseResult {
            capability: cap.into(),
            implementation: impl_id.into(),
            case: case.into(),
            ok: true,
            detail: Some(e),
        },
    }
}

/// Load pilot fixture project from embedded JSON.
pub fn pilot_project() -> Result<Project, ConformanceError> {
    // Local path so the crate packages cleanly on crates.io (no monorepo root).
    let text = include_str!("../fixtures/pilot-json-pipeline.wvx.json");
    serde_json::from_str(text).map_err(|e| ConformanceError::Failed(e.to_string()))
}

/// Run dynamic playground on pilot fixture; return JSON value of serialize.bytes.
pub fn run_dynamic_pilot(
    parse_impl: Option<&str>,
    serialize_impl: Option<&str>,
    input: &[u8],
) -> Result<(serde_json::Value, String, String), ConformanceError> {
    run_dynamic_pilot_ex(parse_impl, serialize_impl, None, input)
}

/// Like [`run_dynamic_pilot`] with optional path_set implementation override.
pub fn run_dynamic_pilot_ex(
    parse_impl: Option<&str>,
    serialize_impl: Option<&str>,
    path_set_impl: Option<&str>,
    input: &[u8],
) -> Result<(serde_json::Value, String, String), ConformanceError> {
    let mut project = pilot_project()?;
    let mut overrides = BTreeMap::new();
    if let Some(p) = parse_impl {
        overrides.insert("parse".into(), p.to_string());
    }
    if let Some(s) = serialize_impl {
        overrides.insert("serialize".into(), s.to_string());
    }
    if let Some(ps) = path_set_impl {
        overrides.insert("path_set".into(), ps.to_string());
    }
    apply_implementation_overrides(&mut project, &overrides);

    let handlers = pilot_sdk_registry();
    let mut seed = WvxValueMap::new();
    seed.insert("bytes".into(), WvxValue::Bytes(input.to_vec()));
    let result = run_project(&project, &handlers, seed)
        .map_err(|e| ConformanceError::Failed(e.to_string()))?;

    let parse_used = result
        .traces
        .iter()
        .find(|t| t.instance_id == "parse")
        .and_then(|t| t.implementation.clone())
        .unwrap_or_default();
    let ser_used = result
        .traces
        .iter()
        .find(|t| t.instance_id == "serialize")
        .and_then(|t| t.implementation.clone())
        .unwrap_or_default();

    let raw = match result.outputs.get("serialize.bytes") {
        Some(WvxValue::Bytes(b)) => b.clone(),
        _ => {
            return Err(ConformanceError::Failed(
                "dynamic run: missing serialize.bytes".into(),
            ))
        }
    };
    let v: serde_json::Value = serde_json::from_slice(&raw)
        .map_err(|e| ConformanceError::Failed(format!("dynamic json: {e}")))?;
    Ok((v, parse_used, ser_used))
}

/// Export pilot to temp dir, cargo run; return JSON stdout.
pub fn run_static_pilot(
    parse_impl: Option<&str>,
    serialize_impl: Option<&str>,
    input: &[u8],
    out_dir: &Path,
) -> Result<serde_json::Value, ConformanceError> {
    run_static_pilot_ex(parse_impl, serialize_impl, None, input, out_dir)
}

/// Like [`run_static_pilot`] with optional path_set implementation override.
pub fn run_static_pilot_ex(
    parse_impl: Option<&str>,
    serialize_impl: Option<&str>,
    path_set_impl: Option<&str>,
    input: &[u8],
    out_dir: &Path,
) -> Result<serde_json::Value, ConformanceError> {
    let mut project = pilot_project()?;
    let mut overrides = BTreeMap::new();
    if let Some(p) = parse_impl {
        overrides.insert("parse".into(), p.to_string());
    }
    if let Some(s) = serialize_impl {
        overrides.insert("serialize".into(), s.to_string());
    }
    if let Some(ps) = path_set_impl {
        overrides.insert("path_set".into(), ps.to_string());
    }
    apply_implementation_overrides(&mut project, &overrides);

    let report = export_to_directory(&project, out_dir, true, Some(input))
        .map_err(|e| ConformanceError::Failed(e.to_string()))?;
    let stdout = report
        .run_stdout
        .ok_or_else(|| ConformanceError::Failed("static export: no stdout".into()))?;
    let v: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| ConformanceError::Failed(format!("static json: {e}; stdout={stdout}")))?;
    Ok(v)
}

/// Compare playground vs static export for pilot combinations.
pub fn golden_dynamic_static(
    parse_impl: Option<&str>,
    serialize_impl: Option<&str>,
    input: &[u8],
) -> Result<GoldenReport, ConformanceError> {
    golden_dynamic_static_ex(parse_impl, serialize_impl, None, input)
}

/// Like [`golden_dynamic_static`] with optional path_set override.
pub fn golden_dynamic_static_ex(
    parse_impl: Option<&str>,
    serialize_impl: Option<&str>,
    path_set_impl: Option<&str>,
    input: &[u8],
) -> Result<GoldenReport, ConformanceError> {
    let (dynamic_json, parse_used, ser_used) =
        run_dynamic_pilot_ex(parse_impl, serialize_impl, path_set_impl, input)?;

    let dir = unique_temp_dir("wvx-golden");
    let static_json = run_static_pilot_ex(parse_impl, serialize_impl, path_set_impl, input, &dir);
    let _ = std::fs::remove_dir_all(&dir);

    match static_json {
        Ok(static_json) => {
            let ok = dynamic_json == static_json;
            Ok(GoldenReport {
                ok,
                dynamic_json,
                static_json: static_json.clone(),
                parse_impl: parse_used,
                serialize_impl: ser_used,
                detail: if ok {
                    None
                } else {
                    Some("dynamic and static JSON values differ".into())
                },
            })
        }
        Err(e) => Ok(GoldenReport {
            ok: false,
            dynamic_json,
            static_json: serde_json::Value::Null,
            parse_impl: parse_used,
            serialize_impl: ser_used,
            detail: Some(e.to_string()),
        }),
    }
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let tid = format!("{:?}", std::thread::current().id()).replace(['(', ')'], "");
    // Unique per process even when tests run in parallel on the same clock tick.
    std::env::temp_dir().join(format!("{prefix}-{nanos}-{seq}-{tid}"))
}

/// Run all default golden combos for the pilot (compact serialize paths).
pub fn run_all_goldens(input: &[u8]) -> Vec<GoldenReport> {
    // (parse, serialize, path_set)
    let combos: Vec<(Option<&str>, Option<&str>, Option<&str>)> = vec![
        (None, None, None), // defaults
        (Some("wvx.reference.json-parse@1"), None, None),
        (Some("json-crate.parse@1"), None, None),
        (None, Some("wvx.reference.json-serialize@1"), None),
        (
            Some("wvx.reference.json-parse@1"),
            Some("wvx.reference.json-serialize@1"),
            None,
        ),
        // path_set swap (Gate A / D for second path_set backend)
        (None, None, Some("serde-json.pointer-set@1")),
    ];
    combos
        .into_iter()
        .map(|(p, s, ps)| {
            golden_dynamic_static_ex(p, s, ps, input).unwrap_or_else(|e| GoldenReport {
                ok: false,
                dynamic_json: serde_json::Value::Null,
                static_json: serde_json::Value::Null,
                parse_impl: p.unwrap_or("default").into(),
                serialize_impl: s.unwrap_or("default").into(),
                detail: Some(e.to_string()),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pilot_capability_conformance() {
        let report = run_pilot_conformance();
        let failed: Vec<_> = report.cases.iter().filter(|c| !c.ok).collect();
        assert!(
            report.ok,
            "conformance failures: {}",
            failed
                .iter()
                .map(|c| format!(
                    "{} / {} / {}: {}",
                    c.capability,
                    c.implementation,
                    c.case,
                    c.detail.as_deref().unwrap_or("?")
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn error_code_family_splits_prefix() {
        assert_eq!(error_code_family("invalid-syntax: EOF"), "invalid-syntax");
        assert_eq!(
            error_code_family("invalid-unicode: bad utf-8"),
            "invalid-unicode"
        );
    }

    #[test]
    fn negative_parse_cases_present() {
        let report = run_pilot_conformance();
        let neg: Vec<_> = report
            .cases
            .iter()
            .filter(|c| c.case.starts_with("neg_"))
            .collect();
        // 15 parse neg × 3 impls + 4 path_set neg × 2 impls = 45 + 8 = 53
        assert!(
            neg.len() >= 45,
            "expected ≥45 negative cases, got {}",
            neg.len()
        );
        assert!(neg.iter().all(|c| c.ok), "negative suite failures");
    }

    #[test]
    fn golden_default_pipeline() {
        let report = golden_dynamic_static(None, None, br#"{"hello":"world"}"#).expect("golden");
        assert!(
            report.ok,
            "detail={:?} dyn={} static={}",
            report.detail, report.dynamic_json, report.static_json
        );
        assert_eq!(report.dynamic_json["hello"], "world");
        assert_eq!(report.dynamic_json["tag"], "loom");
        assert_eq!(report.parse_impl, "serde-json.parse-owned@1");
        assert_eq!(report.serialize_impl, "serde-json.serialize@1");
    }

    #[test]
    fn golden_reference_parse() {
        let report = golden_dynamic_static(
            Some("wvx.reference.json-parse@1"),
            None,
            br#"{"hello":"world"}"#,
        )
        .expect("golden");
        assert!(report.ok, "detail={:?}", report.detail);
        assert_eq!(report.dynamic_json["tag"], "loom");
    }

    #[test]
    fn golden_json_crate_parse() {
        let report =
            golden_dynamic_static(Some("json-crate.parse@1"), None, br#"{"hello":"world"}"#)
                .expect("golden");
        assert!(report.ok, "detail={:?}", report.detail);
        assert_eq!(report.parse_impl, "json-crate.parse@1");
        assert_eq!(report.dynamic_json["tag"], "loom");
    }

    #[test]
    fn golden_all_compact_combos() {
        let reports = run_all_goldens(br#"{"hello":"world"}"#);
        for r in &reports {
            assert!(
                r.ok,
                "combo parse={} ser={}: {:?}",
                r.parse_impl, r.serialize_impl, r.detail
            );
        }
    }

    #[test]
    fn sdk_swap_parse_implementation() {
        let input = br#"{"hello":"world"}"#;
        let (a, parse_a, _) = run_dynamic_pilot(None, None, input).expect("default");
        assert_eq!(parse_a, "serde-json.parse-owned@1");
        assert_eq!(a["hello"], "world");
        assert_eq!(a["tag"], "loom");

        let (b, parse_b, _) =
            run_dynamic_pilot(Some("wvx.reference.json-parse@1"), None, input).expect("swap");
        assert_eq!(parse_b, "wvx.reference.json-parse@1");
        assert_eq!(b["hello"], "world");
        assert_eq!(b["tag"], "loom");
    }

    #[test]
    fn sdk_pretty_serialize_differs_from_compact() {
        let input = br#"{"hello":"world"}"#;
        let handlers = pilot_sdk_registry();
        let mut project = pilot_project().unwrap();
        let mut seed = WvxValueMap::new();
        seed.insert("bytes".into(), WvxValue::Bytes(input.to_vec()));

        let compact = run_project(&project, &handlers, seed.clone()).unwrap();
        let mut overrides = BTreeMap::new();
        overrides.insert(
            "serialize".into(),
            "wvx.reference.json-serialize-pretty@1".into(),
        );
        apply_implementation_overrides(&mut project, &overrides);
        let pretty = run_project(&project, &handlers, seed).unwrap();

        let c = match compact.outputs.get("serialize.bytes") {
            Some(WvxValue::Bytes(b)) => b.clone(),
            _ => panic!("compact bytes"),
        };
        let p = match pretty.outputs.get("serialize.bytes") {
            Some(WvxValue::Bytes(b)) => b.clone(),
            _ => panic!("pretty bytes"),
        };
        assert_ne!(c, p);
        assert!(p.contains(&b'\n'));
        let vc: serde_json::Value = serde_json::from_slice(&c).unwrap();
        let vp: serde_json::Value = serde_json::from_slice(&p).unwrap();
        assert_eq!(vc, vp);
    }
}
