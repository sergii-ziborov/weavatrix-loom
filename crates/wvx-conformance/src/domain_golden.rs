//! Dynamic playground ≡ static export for multi-domain pilots (P1).

use crate::GoldenReport;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use wvx_compiler_rust::export_to_directory;
use wvx_ir::Project;
use wvx_runtime::{apply_implementation_overrides, run_project, HandlerRegistry};
use wvx_types::WvxValue;

fn pilot_registry() -> HandlerRegistry {
    wvx_adapters::register_pilot_plugins();
    wvx_component_sdk::registry_with_pilot_and_plugins()
}

/// Prefer the output sink, never the seeded `input.bytes`.
fn dynamic_sink_bytes(
    project: &Project,
    outputs: &wvx_runtime::WvxValueMap,
) -> Result<Vec<u8>, String> {
    if let Some(WvxValue::Bytes(b)) = outputs.get("output.bytes") {
        return Ok(b.clone());
    }
    if let Some(out_inst) = project
        .instances
        .iter()
        .find(|i| i.capability.id == "io.output.bytes")
    {
        if let Some(binding) = project
            .bindings
            .iter()
            .find(|b| b.to.instance == out_inst.id && b.to.port == "bytes")
        {
            if let Some(WvxValue::Bytes(b)) = outputs.get(&binding.from.to_string()) {
                return Ok(b.clone());
            }
        }
    }
    outputs
        .iter()
        .rev()
        .find_map(|(k, v)| {
            if k.starts_with("input.") {
                return None;
            }
            match v {
                WvxValue::Bytes(b) if k.ends_with(".bytes") || k.ends_with(".digest") => {
                    Some(b.clone())
                }
                _ => None,
            }
        })
        .ok_or_else(|| "dynamic run produced no bytes output".into())
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
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(fixture_rel),
    )
    .map_err(|e| e.to_string())?;
    let mut project: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    apply_implementation_overrides(&mut project, impl_overrides);

    let reg = pilot_registry();
    let mut seed = wvx_runtime::WvxValueMap::new();
    seed.insert("bytes".into(), wvx_types::WvxValue::Bytes(input.to_vec()));
    let dyn_result = run_project(&project, &reg, seed).map_err(|e| e.to_string())?;
    let dyn_bytes = dynamic_sink_bytes(&project, &dyn_result.outputs)?;

    let dir = unique_temp("wvx-domain-golden");
    let export =
        export_to_directory(&project, &dir, true, Some(input)).map_err(|e| e.to_string())?;
    let static_bytes = export.run_output.ok_or_else(|| {
        "export produced no run_output (binary-safe cargo run via WVX_PIPELINE_INPUT_FILE)"
            .to_string()
    })?;
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
                "dynamic len {} != static len {} dyn_b64={} static_b64={}",
                dyn_bytes.len(),
                static_bytes.len(),
                base64_encode(&dyn_bytes),
                base64_encode(&static_bytes)
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

/// Default multi-domain golden set (hash, codec, text, **compression**).
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
        (
            "compress",
            "fixtures/pilot-compress-pipeline.wvx.json",
            b"compression-payload-loom",
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

/// Full dynamic≡static matrix: every registered impl of each domain fixture.
pub fn run_full_dynamic_static_matrix() -> Vec<(String, GoldenReport)> {
    let mut out = Vec::new();
    // Hash
    for impl_id in [
        "sha2.sha256@1",
        "sha2.sha256-streaming@1",
        "sha2.sha256-chunked@1",
        "sha2.sha256-update-all@1",
    ] {
        let mut ov = BTreeMap::new();
        ov.insert("hash".into(), impl_id.into());
        out.push(named_golden(
            &format!("hash:{impl_id}"),
            "fixtures/pilot-hash-pipeline.wvx.json",
            b"abc",
            &ov,
        ));
    }
    // blake3 is a different capability — covered by default hash fixture only when
    // a blake3 pipeline exists; sha256 matrix stays on data.hash.sha256.
    // Compression: every gzip × gunzip pair
    for gzip in [
        "flate2.gzip@1",
        "flate2.gzip-chunked@1",
        "flate2.gzip-oneshot@1",
    ] {
        for gunzip in [
            "flate2.gunzip@1",
            "flate2.gunzip-chunked@1",
            "flate2.gunzip-take@1",
        ] {
            let mut ov = BTreeMap::new();
            ov.insert("gzip".into(), gzip.into());
            ov.insert("gunzip".into(), gunzip.into());
            out.push(named_golden(
                &format!("compress:{gzip}+{gunzip}"),
                "fixtures/pilot-compress-pipeline.wvx.json",
                b"loom-compress",
                &ov,
            ));
        }
    }
    // Codec
    for (label, inst, impl_id) in [
        (
            "hex:wvx.reference.hex-encode@1",
            "hex",
            "wvx.reference.hex-encode@1",
        ),
        (
            "hex:wvx.reference.hex-encode-chunked@1",
            "hex",
            "wvx.reference.hex-encode-chunked@1",
        ),
        (
            "b64:base64.standard-encode@1",
            "b64",
            "base64.standard-encode@1",
        ),
        (
            "b64:wvx.reference.base64-encode@1",
            "b64",
            "wvx.reference.base64-encode@1",
        ),
    ] {
        let mut ov = BTreeMap::new();
        ov.insert(inst.into(), impl_id.into());
        out.push(named_golden(
            label,
            "fixtures/pilot-codec-pipeline.wvx.json",
            b"ab",
            &ov,
        ));
    }
    out.push(named_golden(
        "codec-roundtrip",
        "fixtures/pilot-codec-roundtrip.wvx.json",
        b"hello",
        &BTreeMap::new(),
    ));
    // Text
    for impl_id in ["wvx.reference.text-uppercase@1"] {
        let mut ov = BTreeMap::new();
        ov.insert("upper".into(), impl_id.into());
        out.push(named_golden(
            &format!("text:{impl_id}"),
            "fixtures/pilot-text-pipeline.wvx.json",
            b"Hello Loom",
            &ov,
        ));
    }
    out
}

fn named_golden(
    name: &str,
    fixture: &str,
    input: &[u8],
    ov: &BTreeMap<String, String>,
) -> (String, GoldenReport) {
    let report = golden_fixture_bytes(fixture, input, ov).unwrap_or_else(|e| GoldenReport {
        ok: false,
        dynamic_json: serde_json::Value::Null,
        static_json: serde_json::Value::Null,
        parse_impl: name.into(),
        serialize_impl: String::new(),
        detail: Some(e),
    });
    (name.into(), report)
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

    #[test]
    fn golden_text_pipeline() {
        let r = golden_fixture_bytes(
            "fixtures/pilot-text-pipeline.wvx.json",
            b"Hello Loom",
            &BTreeMap::new(),
        )
        .expect("golden");
        assert!(r.ok, "{:?}", r.detail);
    }

    #[test]
    fn golden_compress_pipeline() {
        let r = golden_fixture_bytes(
            "fixtures/pilot-compress-pipeline.wvx.json",
            b"loom-compress",
            &BTreeMap::new(),
        )
        .expect("golden");
        assert!(r.ok, "{:?}", r.detail);
    }

    #[test]
    fn full_dynamic_static_matrix() {
        let reports = run_full_dynamic_static_matrix();
        let failed: Vec<_> = reports
            .iter()
            .filter(|(_, r)| !r.ok)
            .map(|(n, r)| format!("{n}: {:?}", r.detail))
            .collect();
        assert!(
            failed.is_empty(),
            "dynamic≡static failures:\n{}",
            failed.join("\n")
        );
        assert!(
            reports.iter().any(|(n, _)| n.starts_with("compress:")),
            "compression must be in the matrix"
        );
    }
}
