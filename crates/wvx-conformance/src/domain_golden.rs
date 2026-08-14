//! Dynamic playground ≡ static export for multi-domain pilots (P1).

use crate::GoldenReport;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use wvx_compiler_rust::export_to_directory;
use wvx_ir::Project;
use wvx_runtime::{apply_implementation_overrides, run_project, HandlerRegistry};

fn pilot_registry() -> HandlerRegistry {
    wvx_adapters::register_pilot_plugins();
    wvx_component_sdk::registry_with_pilot_and_plugins()
}

fn unique_temp(prefix: &str) -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{n}"))
}

/// Run fixture in playground and via export; compare raw stdout bytes.
pub fn golden_fixture_bytes(
    fixture_rel: &str,
    input: &[u8],
    impl_overrides: &BTreeMap<String, String>,
) -> Result<GoldenReport, String> {
    let text = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../").join(fixture_rel),
    )
    .map_err(|e| e.to_string())?;
    let mut project: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    apply_implementation_overrides(&mut project, impl_overrides);

    let reg = pilot_registry();
    let mut seed = wvx_runtime::WvxValueMap::new();
    seed.insert(
        "bytes".into(),
        wvx_types::WvxValue::Bytes(input.to_vec()),
    );
    let dyn_result = run_project(&project, &reg, seed).map_err(|e| e.to_string())?;
    // Prefer sink `output.bytes`, then any `*.bytes` / `*.digest`
    let dyn_bytes = dyn_result
        .outputs
        .get("output.bytes")
        .and_then(|v| match v {
            wvx_types::WvxValue::Bytes(b) => Some(b.clone()),
            _ => None,
        })
        .or_else(|| {
            dyn_result.outputs.iter().find_map(|(k, v)| {
                if k.ends_with(".bytes") || k.ends_with(".digest") {
                    match v {
                        wvx_types::WvxValue::Bytes(b) => Some(b.clone()),
                        _ => None,
                    }
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| "dynamic run produced no bytes output".to_string())?;

    let dir = unique_temp("wvx-domain-golden");
    export_to_directory(&project, &dir, true, Some(input)).map_err(|e| e.to_string())?;
    let output = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .env("WVX_PIPELINE_INPUT", String::from_utf8_lossy(input).as_ref())
        .current_dir(&dir)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        let _ = fs::remove_dir_all(&dir);
        return Err(format!(
            "cargo run failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let static_bytes = output.stdout;
    let ok = dyn_bytes == static_bytes;
    let parse_impl = project
        .instances
        .iter()
        .find(|i| i.id != "input" && i.id != "output")
        .and_then(|i| i.implementation.clone())
        .unwrap_or_else(|| "(default)".into());
    let report = GoldenReport {
        ok,
        dynamic_json: serde_json::json!({
            "bytes_b64": base64_encode(&dyn_bytes),
            "len": dyn_bytes.len(),
        }),
        static_json: serde_json::json!({
            "bytes_b64": base64_encode(&static_bytes),
            "len": static_bytes.len(),
        }),
        parse_impl: parse_impl.clone(),
        serialize_impl: parse_impl,
        detail: if ok {
            None
        } else {
            Some(format!(
                "dynamic len {} != static len {}",
                dyn_bytes.len(),
                static_bytes.len()
            ))
        },
    };
    let _ = fs::remove_dir_all(&dir);
    Ok(report)
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    STANDARD.encode(bytes)
}

/// Default multi-domain golden set (hash round-ish, codec roundtrip, text).
pub fn run_domain_goldens() -> Vec<(String, GoldenReport)> {
    let mut out = Vec::new();
    let cases: &[(&str, &str, &[u8])] = &[
        (
            "hash",
            "fixtures/pilot-hash-pipeline.wvx.json",
            b"Weavatrix Loom Domain 2",
        ),
        (
            "codec-roundtrip",
            "fixtures/pilot-codec-roundtrip.wvx.json",
            b"hello codec",
        ),
        (
            "codec-pipeline",
            "fixtures/pilot-codec-pipeline.wvx.json",
            b"ab",
        ),
        (
            "text",
            "fixtures/pilot-text-pipeline.wvx.json",
            b"Hello Loom",
        ),
    ];
    for (name, fixture, input) in cases {
        let report = golden_fixture_bytes(fixture, input, &BTreeMap::new()).unwrap_or_else(|e| {
            GoldenReport {
                ok: false,
                dynamic_json: serde_json::Value::Null,
                static_json: serde_json::Value::Null,
                parse_impl: name.to_string(),
                serialize_impl: String::new(),
                detail: Some(e),
            }
        });
        out.push(((*name).into(), report));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_codec_roundtrip() {
        let r = golden_fixture_bytes(
            "fixtures/pilot-codec-roundtrip.wvx.json",
            b"hello",
            &BTreeMap::new(),
        )
        .expect("golden");
        assert!(r.ok, "{:?}", r.detail);
    }

    #[test]
    fn golden_hash_pipeline() {
        let r = golden_fixture_bytes(
            "fixtures/pilot-hash-pipeline.wvx.json",
            b"abc",
            &BTreeMap::new(),
        )
        .expect("golden");
        assert!(r.ok, "{:?}", r.detail);
    }
}
