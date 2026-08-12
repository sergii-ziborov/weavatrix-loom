//! Gate C pilot harness — Forge economics metrics (static + optional compile).
//!
//! Measures extraction / mapping / compile rates on a **fixed fixture set**.
//! Does **not** claim production Gate C Go without human review (ADR-0007/0010).

use crate::capability_match::{
    match_candidate, MappingKind, OntologyCapability, OntologyPort,
};
use crate::compile_adapter::{compile_adapter_from_draft, CompileAdapterReport};
use crate::draft::draft_adapters_with_ontology;
use crate::extract::{extract_public_api, CandidateKind};
use crate::ForgeError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCCaseExpectation {
    pub package_rel: String,
    pub fn_name: String,
    pub expected_capability: String,
    /// If true, FORGE-008 cargo check should succeed when path deps resolve.
    pub expect_compile: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCCaseResult {
    pub package_rel: String,
    pub fn_name: String,
    pub expected_capability: String,
    pub found: bool,
    pub extractor: Option<String>,
    pub mapping_kind: Option<String>,
    pub mapped_capability: Option<String>,
    pub mapping_correct: bool,
    pub compile: Option<CompileAdapterReport>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCReport {
    pub cases: Vec<GateCCaseResult>,
    /// Fraction of expected functions found by extract.
    pub extraction_recall: f64,
    /// Fraction of found cases with correct capability mapping (reuses_existing).
    pub mapping_accuracy: f64,
    /// Fraction of compile-attempted cases that cargo-check OK.
    pub compile_rate: f64,
    pub false_semantic_mappings: usize,
    pub pilot_go: bool,
    pub notes: Vec<String>,
}

/// Default pilot fixture expectations (JSON vertical + Gate F external).
pub fn pilot_gate_c_expectations() -> Vec<GateCCaseExpectation> {
    vec![
        GateCCaseExpectation {
            package_rel: "crates/wvx-adapter-external-demo".into(),
            fn_name: "upper_parse".into(),
            expected_capability: "data.json.parse@1".into(),
            expect_compile: true,
        },
        GateCCaseExpectation {
            package_rel: "crates/wvx-adapters".into(),
            fn_name: "parse".into(),
            expected_capability: "data.json.parse@1".into(),
            expect_compile: true,
        },
        GateCCaseExpectation {
            package_rel: "crates/wvx-adapters".into(),
            fn_name: "serialize".into(),
            expected_capability: "data.json.serialize@1".into(),
            expect_compile: true,
        },
        GateCCaseExpectation {
            package_rel: "crates/wvx-adapters".into(),
            fn_name: "path_set".into(),
            expected_capability: "data.json.path_set@1".into(),
            expect_compile: true,
        },
        GateCCaseExpectation {
            package_rel: "crates/wvx-component-sdk".into(),
            fn_name: "register_plugin".into(),
            expected_capability: "".into(), // expect new_proposal / no reuse
            expect_compile: false,
        },
    ]
}

/// Pilot ontology (JSON capabilities) for offline Gate C without registry dir.
pub fn pilot_ontology() -> Vec<OntologyCapability> {
    vec![
        OntologyCapability {
            id: "data.json.parse".into(),
            version: "1".into(),
            kind: "transform".into(),
            inputs: vec![OntologyPort {
                id: "bytes".into(),
                ty: "bytes".into(),
            }],
            outputs: vec![OntologyPort {
                id: "value".into(),
                ty: "json_value".into(),
            }],
        },
        OntologyCapability {
            id: "data.json.serialize".into(),
            version: "1".into(),
            kind: "transform".into(),
            inputs: vec![OntologyPort {
                id: "value".into(),
                ty: "json_value".into(),
            }],
            outputs: vec![OntologyPort {
                id: "bytes".into(),
                ty: "bytes".into(),
            }],
        },
        OntologyCapability {
            id: "data.json.path_set".into(),
            version: "1".into(),
            kind: "transform".into(),
            inputs: vec![OntologyPort {
                id: "value".into(),
                ty: "json_value".into(),
            }],
            outputs: vec![OntologyPort {
                id: "value".into(),
                ty: "json_value".into(),
            }],
        },
    ]
}

/// Run Gate C pilot metrics against `workspace_root` (weavatrix-loom monorepo).
pub fn run_gate_c_pilot(
    workspace_root: impl AsRef<Path>,
    ontology: Option<&[OntologyCapability]>,
    run_compile: bool,
) -> Result<GateCReport, ForgeError> {
    let workspace_root = workspace_root.as_ref();
    let ontology_owned;
    let ontology: &[OntologyCapability] = match ontology {
        Some(o) => o,
        None => {
            ontology_owned = pilot_ontology();
            &ontology_owned
        }
    };
    let expectations = pilot_gate_c_expectations();
    let mut cases = Vec::new();
    let compile_root = std::env::temp_dir().join(format!(
        "wvx-gate-c-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));

    for exp in &expectations {
        let pkg = workspace_root.join(&exp.package_rel);
        let mut notes = Vec::new();
        if !pkg.is_dir() {
            cases.push(GateCCaseResult {
                package_rel: exp.package_rel.clone(),
                fn_name: exp.fn_name.clone(),
                expected_capability: exp.expected_capability.clone(),
                found: false,
                extractor: None,
                mapping_kind: None,
                mapped_capability: None,
                mapping_correct: false,
                compile: None,
                notes: vec![format!("package missing: {}", pkg.display())],
            });
            continue;
        }

        let extract = extract_public_api(&pkg)?;
        // For parse/serialize/path_set there may be multiple modules — pick best mapping.
        let fn_candidates: Vec<_> = extract
            .candidates
            .iter()
            .filter(|c| c.kind == CandidateKind::Function && c.name == exp.fn_name)
            .collect();

        if fn_candidates.is_empty() {
            cases.push(GateCCaseResult {
                package_rel: exp.package_rel.clone(),
                fn_name: exp.fn_name.clone(),
                expected_capability: exp.expected_capability.clone(),
                found: false,
                extractor: None,
                mapping_kind: None,
                mapped_capability: None,
                mapping_correct: false,
                compile: None,
                notes: vec!["function not found by extract".into()],
            });
            continue;
        }

        // Prefer candidate that maps correctly if expected set.
        let mut best = fn_candidates[0];
        let mut best_map = match_candidate(&best.name, &best.signature, &best.shape, ontology);
        for c in &fn_candidates[1..] {
            let m = match_candidate(&c.name, &c.signature, &c.shape, ontology);
            if !exp.expected_capability.is_empty()
                && m.capability_key == exp.expected_capability
                && m.kind.reuses_existing()
            {
                best = c;
                best_map = m;
                break;
            }
            if kind_rank(m.kind) > kind_rank(best_map.kind) {
                best = c;
                best_map = m;
            }
        }

        let mapping_correct = if exp.expected_capability.is_empty() {
            // Expect no strong reuse of pilot JSON caps for unrelated fn.
            !best_map.kind.reuses_existing()
                || !best_map.capability_key.starts_with("data.json.")
        } else {
            best_map.capability_key == exp.expected_capability && best_map.kind.reuses_existing()
        };

        if !mapping_correct && best_map.kind.reuses_existing() {
            notes.push("false semantic mapping risk".into());
        }

        let mut compile = None;
        if run_compile && exp.expect_compile && mapping_correct {
            let draft_report =
                draft_adapters_with_ontology(&pkg, Some(&exp.fn_name), ontology)?;
            // Prefer draft whose capability matches expectation.
            let draft = draft_report
                .drafts
                .iter()
                .find(|d| {
                    d.candidate_name == exp.fn_name
                        && (exp.expected_capability.is_empty()
                            || d.capability_id == exp.expected_capability)
                })
                .or_else(|| {
                    draft_report
                        .drafts
                        .iter()
                        .find(|d| d.capability_id == exp.expected_capability)
                })
                .or_else(|| draft_report.drafts.first());
            if let Some(d) = draft {
                let out = compile_root.join(sanitize(&d.implementation_id));
                match compile_adapter_from_draft(
                    &pkg,
                    &draft_report.package_name,
                    d,
                    &out,
                    true,
                ) {
                    Ok(r) => {
                        notes.push(format!("compile status={}", r.status));
                        compile = Some(r);
                    }
                    Err(e) => notes.push(format!("compile error: {e}")),
                }
            }
        }

        cases.push(GateCCaseResult {
            package_rel: exp.package_rel.clone(),
            fn_name: exp.fn_name.clone(),
            expected_capability: exp.expected_capability.clone(),
            found: true,
            extractor: Some(best.extractor.clone()),
            mapping_kind: Some(best_map.kind.as_str().into()),
            mapped_capability: Some(best_map.capability_key.clone()),
            mapping_correct,
            compile,
            notes,
        });
    }

    let found_n = cases.iter().filter(|c| c.found).count();
    let extraction_recall = cases.len().max(1) as f64;
    let extraction_recall = found_n as f64 / extraction_recall;

    let mapped_n = cases.iter().filter(|c| c.found).count().max(1);
    let correct_n = cases.iter().filter(|c| c.mapping_correct).count();
    let mapping_accuracy = correct_n as f64 / mapped_n as f64;

    let compile_cases: Vec<_> = cases
        .iter()
        .filter(|c| c.compile.as_ref().and_then(|r| r.compile_ok).is_some())
        .collect();
    let compile_ok = compile_cases
        .iter()
        .filter(|c| c.compile.as_ref().and_then(|r| r.compile_ok) == Some(true))
        .count();
    let compile_rate = if compile_cases.is_empty() {
        0.0
    } else {
        compile_ok as f64 / compile_cases.len() as f64
    };

    let false_semantic_mappings = cases
        .iter()
        .filter(|c| {
            c.found
                && !c.mapping_correct
                && c.mapping_kind.as_deref() == Some("exact_shape")
        })
        .count();

    // Pilot Go: high mapping accuracy + majority compile on expected compile cases.
    let pilot_go = extraction_recall >= 0.8
        && mapping_accuracy >= 0.8
        && (compile_cases.is_empty() || compile_rate >= 0.5)
        && false_semantic_mappings == 0;

    let _ = std::fs::remove_dir_all(&compile_root);

    Ok(GateCReport {
        cases,
        extraction_recall,
        mapping_accuracy,
        compile_rate,
        false_semantic_mappings,
        pilot_go,
        notes: vec![
            "Gate C pilot harness — fixture metrics only; not public Registry Go.".into(),
            "AI/heuristic mapping never sets evidence pass (ADR-0010).".into(),
            format!(
                "thresholds: extraction_recall≥0.8 mapping_accuracy≥0.8 compile_rate≥0.5 false_map=0"
            ),
        ],
    })
}

fn kind_rank(k: MappingKind) -> u8 {
    match k {
        MappingKind::ExactShape => 4,
        MappingKind::CompatibleShape => 3,
        MappingKind::FamilyHint => 2,
        MappingKind::NewProposal => 1,
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Resolve monorepo root from this crate's manifest (tests / CLI default).
pub fn default_workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_c_pilot_metrics() {
        let root = default_workspace_root();
        // Compile is slower; still run for real Gate C signal.
        let report = run_gate_c_pilot(&root, None, true).unwrap();
        assert!(
            report.extraction_recall >= 0.8,
            "recall={}",
            report.extraction_recall
        );
        assert!(
            report.mapping_accuracy >= 0.8,
            "mapping={} cases={:?}",
            report.mapping_accuracy,
            report
                .cases
                .iter()
                .map(|c| (
                    &c.fn_name,
                    c.mapping_correct,
                    &c.mapped_capability,
                    &c.expected_capability
                ))
                .collect::<Vec<_>>()
        );
        assert_eq!(report.false_semantic_mappings, 0);
        assert!(
            report.compile_rate >= 0.5,
            "compile_rate={} cases={:?}",
            report.compile_rate,
            report
                .cases
                .iter()
                .filter_map(|c| c.compile.as_ref())
                .map(|r| (&r.implementation_id, r.compile_ok, r.compile_log.as_ref().map(|s| s.chars().take(200).collect::<String>())))
                .collect::<Vec<_>>()
        );
        assert!(report.pilot_go, "pilot_go false: {:?}", report.notes);
    }
}
