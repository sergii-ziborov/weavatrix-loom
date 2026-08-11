//! Pilot conformance suite and dynamic≡static golden checks.
//!
//! Not a full evidence/admission pipeline — focused on Transform MVP:
//! every registered pilot implementation of a capability must agree on
//! shared vectors (semantic equality for JSON), and playground output
//! must match the exported static pipeline on the pilot fixture.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use wvx_compiler_rust::export_to_directory;
use wvx_ir::Project;
use wvx_runtime::{
    apply_implementation_overrides, run_project, HandlerRegistry, WvxValueMap,
};
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
        ("array", br#"[1,"x",null]"#.to_vec(), serde_json::json!([1, "x", null])),
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
    &[
        "wvx.reference.path-set@1",
        "serde-json.pointer-set@1",
    ]
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
    ]
}

fn serialize_impls_compact() -> &'static [&'static str] {
    &[
        "serde-json.serialize@1",
        "wvx.reference.json-serialize@1",
    ]
}

/// Run capability-level conformance for pilot JSON handlers.
pub fn run_pilot_conformance() -> ConformanceReport {
    let reg = HandlerRegistry::with_pilot();
    let mut cases = Vec::new();

    // --- parse ---
    for impl_id in parse_impls() {
        for (name, bytes, expected) in parse_vectors() {
            let result = conform_parse(&reg, impl_id, name, bytes.as_slice(), &expected);
            cases.push(result);
        }
    }

    // --- serialize (compact): re-parse bytes must equal original JSON ---
    let sample = serde_json::json!({"hello":"world","n":1,"ok":true});
    for impl_id in serialize_impls_compact() {
        cases.push(conform_serialize_roundtrip(&reg, impl_id, "roundtrip_object", &sample));
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

    let ok = cases.iter().all(|c| c.ok);
    ConformanceReport { ok, cases }
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

/// Load pilot fixture project from embedded JSON.
pub fn pilot_project() -> Result<Project, ConformanceError> {
    let text = include_str!("../../../fixtures/pilot-json-pipeline.wvx.json");
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

    let handlers = HandlerRegistry::with_pilot();
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
    let static_json =
        run_static_pilot_ex(parse_impl, serialize_impl, path_set_impl, input, &dir);
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
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}-{nanos}"))
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
    fn golden_default_pipeline() {
        let report =
            golden_dynamic_static(None, None, br#"{"hello":"world"}"#).expect("golden");
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
        let report = golden_dynamic_static(
            Some("json-crate.parse@1"),
            None,
            br#"{"hello":"world"}"#,
        )
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
}
