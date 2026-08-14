//! Profile-driven conformance runner (P1).
//!
//! ```text
//! Profile → vectors → implementation set → execute → normalize errors → compare → CaseResult
//! ```
//!
//! Does not hard-code capability families: ports and vectors come from the
//! profile document under `registry-dev/profiles/`.

use crate::{error_code_family, ConformanceCaseResult, ConformanceReport};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use wvx_registry_client::{
    load_profile, suite_digest_for_profile, CaseResult, ConformanceProfileDoc,
};
use wvx_runtime::{HandlerRegistry, WvxValueMap};
use wvx_types::WvxValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileRunReport {
    pub ok: bool,
    pub profile_id: String,
    pub capability_key: String,
    pub suite_digest: String,
    pub implementations: Vec<String>,
    pub cases: Vec<ConformanceCaseResult>,
    /// Flattened case results suitable for EvidenceArtifact mint.
    pub case_results: Vec<CaseResult>,
    pub guarantees_checked: Vec<String>,
}

/// Run a versioned profile against all registered handlers for its capability.
pub fn run_profile_conformance(
    registry_root: &Path,
    handlers: &HandlerRegistry,
    profile_id: &str,
) -> Result<ProfileRunReport, String> {
    let (doc, _) = load_profile(registry_root, profile_id).map_err(|e| e.to_string())?;
    run_profile_doc(handlers, &doc)
}

/// Run with an already-loaded profile document.
pub fn run_profile_doc(
    handlers: &HandlerRegistry,
    doc: &ConformanceProfileDoc,
) -> Result<ProfileRunReport, String> {
    let cap = doc.capability_key.clone();
    let suite_digest = suite_digest_for_profile(doc);
    let mut impls = handlers.list_implementations(&cap);
    impls.sort();
    if impls.is_empty() {
        return Err(format!(
            "no registered implementations for capability `{cap}` (profile {})",
            doc.id
        ));
    }

    let mut cases = Vec::new();
    let mut case_results = Vec::new();
    let mut guarantees = doc.guarantees.clone();
    if guarantees.is_empty() {
        guarantees.push("shared vectors agree / negatives fail".into());
    }

    // Positive vectors
    for vec in &doc.vectors {
        let id = vec
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed")
            .to_string();
        let expect_ok = vec
            .get("expect_ok")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let input = decode_input_b64(vec)?;
        let expect_out = vec
            .get("expect_output_b64")
            .and_then(|v| v.as_str())
            .and_then(|s| decode_b64(s).ok());

        // Collect outputs for multi-impl equality when no golden output
        let mut outputs: Vec<(String, Vec<u8>)> = Vec::new();

        for impl_id in &impls {
            let result = execute_bytes_transform(handlers, &cap, impl_id, &input);
            match result {
                Ok(out) => {
                    if !expect_ok {
                        let cr = ConformanceCaseResult {
                            capability: cap.clone(),
                            implementation: impl_id.clone(),
                            case: format!("pos:{id}"),
                            ok: false,
                            detail: Some("expected failure but succeeded".into()),
                        };
                        case_results.push(to_case(&cr, "positive", None));
                        cases.push(cr);
                        continue;
                    }
                    if let Some(ref golden) = expect_out {
                        let ok = &out == golden;
                        let cr = ConformanceCaseResult {
                            capability: cap.clone(),
                            implementation: impl_id.clone(),
                            case: format!("pos:{id}"),
                            ok,
                            detail: if ok {
                                None
                            } else {
                                Some(format!(
                                    "output len {} != expected {}",
                                    out.len(),
                                    golden.len()
                                ))
                            },
                        };
                        case_results.push(to_case(&cr, "positive", None));
                        cases.push(cr);
                    } else {
                        outputs.push((impl_id.clone(), out));
                        // provisional ok; equality checked after loop
                    }
                }
                Err(e) => {
                    if expect_ok {
                        let cr = ConformanceCaseResult {
                            capability: cap.clone(),
                            implementation: impl_id.clone(),
                            case: format!("pos:{id}"),
                            ok: false,
                            detail: Some(e),
                        };
                        case_results.push(to_case(&cr, "positive", None));
                        cases.push(cr);
                    } else {
                        let cr = ConformanceCaseResult {
                            capability: cap.clone(),
                            implementation: impl_id.clone(),
                            case: format!("pos:{id}"),
                            ok: true,
                            detail: Some(format!("failed as expected: {e}")),
                        };
                        case_results.push(to_case(&cr, "positive", None));
                        cases.push(cr);
                    }
                }
            }
        }

        // Multi-impl equality for vectors without golden output
        if expect_ok && expect_out.is_none() && !outputs.is_empty() {
            let reference = &outputs[0].1;
            for (impl_id, bytes) in &outputs {
                let ok = bytes == reference;
                let cr = ConformanceCaseResult {
                    capability: cap.clone(),
                    implementation: impl_id.clone(),
                    case: format!("pos:{id}:eq"),
                    ok,
                    detail: if ok {
                        Some(format!("{} impls bit-equal ({} bytes)", outputs.len(), reference.len()))
                    } else {
                        Some(format!(
                            "bit mismatch vs {} (lens {} vs {})",
                            outputs[0].0,
                            bytes.len(),
                            reference.len()
                        ))
                    },
                };
                case_results.push(to_case(&cr, "positive", None));
                cases.push(cr);
            }
        }
    }

    // Negative vectors
    for vec in &doc.negative_vectors {
        let id = vec
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("neg")
            .to_string();
        let input = decode_input_b64(vec)?;
        let expect_family = vec
            .get("expect_error_family")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        for impl_id in &impls {
            let result = execute_bytes_transform(handlers, &cap, impl_id, &input);
            match result {
                Ok(_) => {
                    let cr = ConformanceCaseResult {
                        capability: cap.clone(),
                        implementation: impl_id.clone(),
                        case: format!("neg:{id}"),
                        ok: false,
                        detail: Some("expected error, got success".into()),
                    };
                    case_results.push(to_case(&cr, "negative", expect_family.clone()));
                    cases.push(cr);
                }
                Err(e) => {
                    let family = error_code_family(&e);
                    let ok = match &expect_family {
                        Some(want) => family == want.as_str() || e.contains(want.as_str()),
                        None => true,
                    };
                    let cr = ConformanceCaseResult {
                        capability: cap.clone(),
                        implementation: impl_id.clone(),
                        case: format!("neg:{id}"),
                        ok,
                        detail: if ok {
                            Some(format!("failed with family `{family}`"))
                        } else {
                            Some(format!(
                                "error family `{family}` != expected `{}` ({e})",
                                expect_family.as_deref().unwrap_or("?")
                            ))
                        },
                    };
                    case_results.push(to_case(&cr, "negative", expect_family.clone()));
                    cases.push(cr);
                }
            }
        }
    }

    let ok = cases.iter().all(|c| c.ok);
    Ok(ProfileRunReport {
        ok,
        profile_id: doc.id.clone(),
        capability_key: cap,
        suite_digest,
        implementations: impls,
        cases,
        case_results,
        guarantees_checked: guarantees,
    })
}

/// Run several well-known multi-domain profiles and fold into one report.
pub fn run_multi_domain_profiles(
    registry_root: &Path,
    handlers: &HandlerRegistry,
) -> ConformanceReport {
    let profiles = [
        "sha256-fips180-4-v1",
        "hex-rfc-encode-v1",
        "hex-rfc-decode-v1",
        "base64-rfc4648-standard-v1",
        "json-rfc8259-core-v1",
    ];
    let mut cases = Vec::new();
    for pid in profiles {
        match run_profile_conformance(registry_root, handlers, pid) {
            Ok(r) => {
                cases.extend(r.cases);
            }
            Err(e) => {
                // Profile may target caps without handlers — soft skip with note
                cases.push(ConformanceCaseResult {
                    capability: pid.into(),
                    implementation: "(profile)".into(),
                    case: "load".into(),
                    ok: false,
                    detail: Some(e),
                });
            }
        }
    }
    let ok = cases.iter().all(|c| c.ok);
    ConformanceReport { ok, cases }
}

fn to_case(
    c: &ConformanceCaseResult,
    kind: &str,
    expected_error_family: Option<String>,
) -> CaseResult {
    CaseResult {
        case_id: format!("{}:{}:{}", c.implementation, c.case, kind),
        kind: kind.into(),
        ok: c.ok,
        detail: c.detail.clone(),
        expected_error_family,
    }
}

fn decode_input_b64(vec: &serde_json::Value) -> Result<Vec<u8>, String> {
    let s = vec
        .get("input_b64")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    decode_b64(s)
}

fn decode_b64(s: &str) -> Result<Vec<u8>, String> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    use base64::{engine::general_purpose::STANDARD, Engine};
    STANDARD
        .decode(s.trim())
        .map_err(|e| format!("base64 decode: {e}"))
}

/// Execute bytes-in → bytes-or-digest-out for a capability.
fn execute_bytes_transform(
    handlers: &HandlerRegistry,
    cap: &str,
    impl_id: &str,
    input: &[u8],
) -> Result<Vec<u8>, String> {
    let handler = handlers
        .resolve(cap, Some(impl_id))
        .map_err(|e| e.to_string())?;
    let mut inputs = WvxValueMap::new();
    // Common input port names
    inputs.insert("bytes".into(), WvxValue::Bytes(input.to_vec()));
    let out = handler.execute(&inputs, &BTreeMap::new())?;
    // Prefer digest, then bytes, then value (json serialized)
    if let Some(WvxValue::Bytes(b)) = out.get("digest") {
        return Ok(b.clone());
    }
    if let Some(WvxValue::Bytes(b)) = out.get("bytes") {
        return Ok(b.clone());
    }
    if let Some(WvxValue::Json(v)) = out.get("value") {
        return serde_json::to_vec(v).map_err(|e| e.to_string());
    }
    Err(format!("no bytes/digest/value output ports: {:?}", out.keys().collect::<Vec<_>>()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn handlers() -> HandlerRegistry {
        wvx_adapters::register_pilot_plugins();
        wvx_component_sdk::registry_with_pilot_and_plugins()
    }

    #[test]
    fn profile_sha256_runs() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry-dev");
        let h = handlers();
        let r = run_profile_conformance(&root, &h, "sha256-fips180-4-v1").expect("run");
        assert!(
            r.ok,
            "failures: {:?}",
            r.cases.iter().filter(|c| !c.ok).collect::<Vec<_>>()
        );
        assert!(r.implementations.len() >= 3);
    }

    #[test]
    fn profile_hex_encode_runs() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry-dev");
        let h = handlers();
        let r = run_profile_conformance(&root, &h, "hex-rfc-encode-v1").expect("run");
        assert!(
            r.ok,
            "failures: {:?}",
            r.cases.iter().filter(|c| !c.ok).collect::<Vec<_>>()
        );
    }

    #[test]
    fn profile_base64_runs() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry-dev");
        let h = handlers();
        let r = run_profile_conformance(&root, &h, "base64-rfc4648-standard-v1").expect("run");
        assert!(
            r.ok,
            "failures: {:?}",
            r.cases.iter().filter(|c| !c.ok).collect::<Vec<_>>()
        );
    }
}
