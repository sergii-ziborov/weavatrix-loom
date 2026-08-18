//! Schema contract helpers for Loom wire formats.
//!
//! Hand-authored JSON Schemas under `schemas/` are checked against:
//! 1. Fixture documents (required keys present)
//! 2. Serde roundtrips of canonical Rust types
//!
//! Full automatic generation (schemars) can replace hand schemas later;
//! until then, these tests keep schema and code from drifting.

pub mod draft2020;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("io {0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("parse {0}: {1}")]
    Parse(PathBuf, String),
    #[error("{0}")]
    Fail(String),
}

/// Monorepo root containing `schemas/` and `fixtures/`.
pub fn monorepo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn schemas_dir() -> PathBuf {
    monorepo_root().join("schemas")
}

pub fn load_json(path: &Path) -> Result<Value, ContractError> {
    let text = fs::read_to_string(path).map_err(|e| ContractError::Io(path.to_path_buf(), e))?;
    serde_json::from_str(&text).map_err(|e| ContractError::Parse(path.to_path_buf(), e.to_string()))
}

/// Extract top-level `required` string list from a draft-2020-12 schema object.
pub fn schema_required(schema: &Value) -> Result<Vec<String>, ContractError> {
    let req = schema
        .get("required")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ContractError::Fail("schema missing required array".into()))?;
    let mut out = Vec::new();
    for item in req {
        let s = item
            .as_str()
            .ok_or_else(|| ContractError::Fail("required entry not string".into()))?;
        out.push(s.to_string());
    }
    Ok(out)
}

/// Assert every schema-required top-level key is present on `doc`.
pub fn assert_required_fields(
    schema: &Value,
    doc: &Value,
    label: &str,
) -> Result<(), ContractError> {
    let required = schema_required(schema)?;
    let obj = doc
        .as_object()
        .ok_or_else(|| ContractError::Fail(format!("{label}: document is not an object")))?;
    let mut missing = Vec::new();
    for key in &required {
        if !obj.contains_key(key) {
            missing.push(key.clone());
        }
    }
    if !missing.is_empty() {
        return Err(ContractError::Fail(format!(
            "{label}: missing required fields: {}",
            missing.join(", ")
        )));
    }
    Ok(())
}

/// Serialize → deserialize and compare via JSON equality.
pub fn roundtrip_json<T>(value: &T) -> Result<T, ContractError>
where
    T: Serialize + DeserializeOwned,
{
    let json =
        serde_json::to_value(value).map_err(|e| ContractError::Fail(format!("serialize: {e}")))?;
    let back: T = serde_json::from_value(json.clone())
        .map_err(|e| ContractError::Fail(format!("deserialize: {e}")))?;
    let again = serde_json::to_value(&back)
        .map_err(|e| ContractError::Fail(format!("re-serialize: {e}")))?;
    if json != again {
        return Err(ContractError::Fail(format!(
            "roundtrip JSON mismatch:\n left={json}\nright={again}"
        )));
    }
    Ok(back)
}

/// Load schema file by name under monorepo `schemas/`.
pub fn load_schema(name: &str) -> Result<Value, ContractError> {
    let path = schemas_dir().join(name);
    load_json(&path)
}

/// Validate a document path against schema required fields.
pub fn validate_doc_path(
    schema_name: &str,
    doc_path: &Path,
    label: &str,
) -> Result<(), ContractError> {
    let schema = load_schema(schema_name)?;
    let doc = load_json(doc_path)?;
    assert_required_fields(&schema, &doc, label)
}

/// Full Draft 2020-12 evaluation of a document against a named schema.
pub fn validate_doc_draft2020(
    schema_name: &str,
    doc_path: &Path,
    label: &str,
) -> Result<(), ContractError> {
    let schema = load_schema(schema_name)?;
    let doc = load_json(doc_path)?;
    draft2020::validate_instance(&schema, &doc, label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use wvx_ir::{
        Capability, CapabilityRef, Implementation, ImplementationEvidence, ImplementationSource,
        Instance, PortPath, PortSpec, Project, PROJECT_SCHEMA_VERSION,
    };
    use wvx_project_graph::{GraphOp, GraphPatch};
    use wvx_registry_client::{
        CaseResult, EvidenceArtifact, EvidenceDigests, EvidenceEnvironment, SuiteResult,
        EVIDENCE_SCHEMA,
    };
    use wvx_types::TypeRef;

    #[test]
    fn project_schema_required_matches_fixture() {
        let fixture = monorepo_root().join("fixtures/pilot-json-pipeline.wvx.json");
        validate_doc_path("wvx.project.v0.1.json", &fixture, "pilot-json").unwrap();
    }

    #[test]
    fn capability_schema_required_matches_registry() {
        let cap = monorepo_root().join("registry-dev/capabilities/data.json.parse@1.json");
        validate_doc_path("wvx.capability.v0.1.json", &cap, "parse-cap").unwrap();
    }

    #[test]
    fn implementation_schema_required_matches_registry() {
        let imp =
            monorepo_root().join("registry-dev/implementations/serde-json.parse-owned@1.json");
        validate_doc_path("wvx.implementation.v0.1.json", &imp, "serde-parse").unwrap();
    }

    #[test]
    fn evidence_v02_schema_required_matches_sample() {
        let art =
            monorepo_root().join("registry-dev/evidence/artifacts/serde-json.parse-owned@1.json");
        validate_doc_path("wvx.evidence_artifact.v0.2.json", &art, "evidence-v2").unwrap();
    }

    #[test]
    fn project_rust_roundtrip() {
        let mut p = Project::new("contract", "Contract");
        p.schema_version = PROJECT_SCHEMA_VERSION.into();
        p.capabilities.push(Capability {
            id: "io.input.bytes".into(),
            version: "1".into(),
            kind: "io".into(),
            inputs: vec![],
            outputs: vec![PortSpec {
                id: "bytes".into(),
                ty: TypeRef::Bytes,
                required: true,
            }],
            errors: vec![],
            effects: vec![],
        });
        p.instances.push(Instance {
            id: "input".into(),
            capability: CapabilityRef::new("io.input.bytes", "1"),
            implementation: None,
            config: BTreeMap::new(),
            ui: None,
        });
        p.entrypoint = Some("input".into());
        let back = roundtrip_json(&p).unwrap();
        assert_eq!(back.id, "contract");
        assert_eq!(back.capabilities.len(), 1);
    }

    #[test]
    fn implementation_rust_roundtrip() {
        use wvx_ir::SourceRef;
        let imp = Implementation {
            id: "demo".into(),
            version: "1".into(),
            capability: CapabilityRef::new("data.json.parse", "1"),
            source: ImplementationSource {
                kind: "crates-io".into(),
                package: "serde_json".into(),
                package_version: "1".into(),
                notes: None,
            },
            adapter: None,
            status: Default::default(),
            evidence: ImplementationEvidence::default(),
            notes: None,
            sdk: None,
            conformance_profile: Some("json-rfc8259-core-v1".into()),
            evidence_artifact: Some("evidence/artifacts/demo@1.json".into()),
            source_ref: Some(SourceRef {
                provider: "weavatrix".into(),
                entity_id: "wvx:demo:fn:parse".into(),
                revision: Some("r1".into()),
                path: Some("src/lib.rs".into()),
                notes: None,
            }),
        };
        let back = roundtrip_json(&imp).unwrap();
        assert_eq!(back.full_id(), "demo@1");
        assert_eq!(
            back.conformance_profile.as_deref(),
            Some("json-rfc8259-core-v1")
        );
        assert_eq!(
            back.source_ref.as_ref().map(|s| s.entity_id.as_str()),
            Some("wvx:demo:fn:parse")
        );
    }

    #[test]
    fn implementation_schema_source_ref_on_sample() {
        let imp =
            monorepo_root().join("registry-dev/implementations/serde-json.parse-owned@1.json");
        let doc = load_json(&imp).unwrap();
        assert!(doc.get("source_ref").is_some());
        assert_eq!(doc["source_ref"]["provider"], "manual");
    }

    #[test]
    fn facts_schema_required_matches_sample() {
        let facts = monorepo_root().join("fixtures/weavatrix-facts-sample.json");
        validate_doc_path("wvx.facts.v0.1.json", &facts, "facts-sample").unwrap();
    }

    #[test]
    fn graph_patch_rust_roundtrip() {
        let patch = GraphPatch {
            ops: vec![GraphOp::Connect {
                from: PortPath::new("a", "out"),
                to: PortPath::new("b", "in"),
            }],
            rationale: "contract".into(),
            unresolved: vec![],
            base_revision: Some(3),
            patch_id: Some("p1".into()),
        };
        let back = roundtrip_json(&patch).unwrap();
        assert_eq!(back.base_revision, Some(3));
        assert_eq!(back.ops.len(), 1);
    }

    #[test]
    fn evidence_artifact_v02_rust_roundtrip() {
        let art = EvidenceArtifact {
            schema_version: EVIDENCE_SCHEMA.into(),
            implementation_id: "demo@1".into(),
            capability_key: "data.json.parse@1".into(),
            conformance_profile: "json-rfc8259-core-v1".into(),
            subject_digest: "sha256:abc".into(),
            profile_suite_digest: "sha256:def".into(),
            suite_results: vec![SuiteResult {
                profile: "json-rfc8259-core-v1".into(),
                suite_digest: "sha256:def".into(),
                passed: true,
                cases_ok: 1,
                cases_total: 1,
                notes: vec![],
            }],
            axes: ImplementationEvidence::default(),
            recorded_at_unix: 1,
            notes: vec![],
            digests: EvidenceDigests {
                implementation_source_tree: "sha256:1".into(),
                upstream_package: "sha256:2".into(),
                cargo_lock: "sha256:3".into(),
                adapter_source: "sha256:4".into(),
                capability_contract: "sha256:5".into(),
                profile: "sha256:6".into(),
                suite: "sha256:def".into(),
                subject: "sha256:abc".into(),
                package_checksum: "sha256:7".into(),
                source_ref_revision: "sha256:absent".into(),
                profile_case_ids: "sha256:8".into(),
            },
            environment: EvidenceEnvironment {
                target: "test".into(),
                toolchain: "rustc".into(),
                features: vec![],
                runner_identity: "contract".into(),
            },
            case_results: vec![CaseResult {
                case_id: "c0".into(),
                kind: "positive".into(),
                ok: true,
                detail: None,
                expected_error_family: None,
            }],
        };
        let schema = load_schema("wvx.evidence_artifact.v0.2.json").unwrap();
        let doc = serde_json::to_value(&art).unwrap();
        assert_required_fields(&schema, &doc, "minted-v2").unwrap();
        let back = roundtrip_json(&art).unwrap();
        assert_eq!(back.schema_version, EVIDENCE_SCHEMA);
        assert_eq!(back.case_results.len(), 1);
    }

    #[test]
    fn fixture_projects_satisfy_project_schema() {
        let schema = load_schema("wvx.project.v0.1.json").unwrap();
        let dir = monorepo_root().join("fixtures");
        let mut checked = 0usize;
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy();
            if !name.contains("pilot-") || !name.ends_with(".wvx.json") {
                continue;
            }
            let doc = load_json(&path).unwrap();
            assert_required_fields(&schema, &doc, &name).unwrap();
            // Full Rust deserialize contract
            let _: Project = serde_json::from_value(doc).unwrap();
            checked += 1;
        }
        assert!(checked >= 4, "expected ≥4 pilot fixtures, got {checked}");
    }

    #[test]
    fn schema_const_versions_documented() {
        // Keep schema files' const schema_version aligned with known constants.
        let project = load_schema("wvx.project.v0.1.json").unwrap();
        assert_eq!(
            project["properties"]["schema_version"]["const"],
            "wvx.project.v0.1"
        );
        let ev = load_schema("wvx.evidence_artifact.v0.2.json").unwrap();
        assert_eq!(
            ev["properties"]["schema_version"]["const"],
            "wvx.evidence.v0.2"
        );
        assert_eq!(PROJECT_SCHEMA_VERSION, "wvx.project.v0.1");
        assert_eq!(EVIDENCE_SCHEMA, "wvx.evidence.v0.2");
    }

    #[test]
    fn draft2020_validates_project_and_facts() {
        validate_doc_draft2020(
            "wvx.project.v0.1.json",
            &monorepo_root().join("fixtures/pilot-json-pipeline.wvx.json"),
            "pilot-json",
        )
        .unwrap();
        validate_doc_draft2020(
            "wvx.facts.v0.1.json",
            &monorepo_root().join("fixtures/weavatrix-facts-sample.json"),
            "facts-v01",
        )
        .unwrap();
        let ev_schema = load_schema("wvx.evidence_artifact.v0.2.json").unwrap();
        let ev_doc = load_json(
            &monorepo_root().join("registry-dev/evidence/artifacts/serde-json.parse-owned@1.json"),
        )
        .unwrap();
        // Sample artifact may predate new digest fields; required keys still hold.
        assert_required_fields(&ev_schema, &ev_doc, "sample-artifact").unwrap();
    }

    #[test]
    fn draft2020_rejects_missing_const() {
        let schema = load_schema("wvx.project.v0.1.json").unwrap();
        let mut doc =
            load_json(&monorepo_root().join("fixtures/pilot-json-pipeline.wvx.json")).unwrap();
        doc["schema_version"] = serde_json::json!("wrong");
        let err = crate::draft2020::validate_instance(&schema, &doc, "bad").unwrap_err();
        assert!(err.to_string().contains("const"), "{err}");
    }
}
