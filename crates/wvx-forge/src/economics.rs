//! Gate C pilot harness — Forge economics metrics (static + optional compile).
//!
//! Measures extraction / mapping / compile rates on a **fixed fixture set**.
//! Does **not** claim production Gate C Go without human review (ADR-0007/0010).

use crate::capability_match::{match_candidate, MappingKind, OntologyCapability, OntologyPort};
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
    /// Estimated human minutes for review on this fixture set (pilot heuristic).
    /// Full Gate C requires measured human-minutes on **external** crates.
    #[serde(default)]
    pub human_minutes_estimate: f64,
    /// True only when run against external packages (not monorepo fixtures).
    #[serde(default)]
    pub external_crates: bool,
}

/// Default pilot fixture expectations (JSON vertical + Gate F external).
pub fn pilot_gate_c_expectations() -> Vec<GateCCaseExpectation> {
    vec![
        GateCCaseExpectation {
            package_rel: "crates/wvx-adapter-external-demo".into(),
            fn_name: "parse".into(),
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

fn bytes_in_out(id: &str) -> OntologyCapability {
    OntologyCapability {
        id: id.into(),
        version: "1".into(),
        kind: "transform".into(),
        inputs: vec![OntologyPort {
            id: "bytes".into(),
            ty: "bytes".into(),
        }],
        outputs: vec![OntologyPort {
            id: "bytes".into(),
            ty: "bytes".into(),
        }],
    }
}

/// Pilot ontology (JSON + hash + compress + codec + text) for offline Gate C.
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
        OntologyCapability {
            id: "data.hash.sha256".into(),
            version: "1".into(),
            kind: "transform".into(),
            inputs: vec![OntologyPort {
                id: "bytes".into(),
                ty: "bytes".into(),
            }],
            outputs: vec![OntologyPort {
                id: "digest".into(),
                ty: "bytes".into(),
            }],
        },
        OntologyCapability {
            id: "data.hash.blake3".into(),
            version: "1".into(),
            kind: "transform".into(),
            inputs: vec![OntologyPort {
                id: "bytes".into(),
                ty: "bytes".into(),
            }],
            outputs: vec![OntologyPort {
                id: "digest".into(),
                ty: "bytes".into(),
            }],
        },
        OntologyCapability {
            id: "data.compress.gzip".into(),
            version: "1".into(),
            kind: "transform".into(),
            inputs: vec![OntologyPort {
                id: "bytes".into(),
                ty: "bytes".into(),
            }],
            outputs: vec![OntologyPort {
                id: "bytes".into(),
                ty: "bytes".into(),
            }],
        },
        OntologyCapability {
            id: "data.compress.gunzip".into(),
            version: "1".into(),
            kind: "transform".into(),
            inputs: vec![OntologyPort {
                id: "bytes".into(),
                ty: "bytes".into(),
            }],
            outputs: vec![OntologyPort {
                id: "bytes".into(),
                ty: "bytes".into(),
            }],
        },
        bytes_in_out("data.codec.hex_encode"),
        bytes_in_out("data.codec.hex_decode"),
        bytes_in_out("data.codec.base64_encode"),
        bytes_in_out("data.codec.base64_decode"),
        bytes_in_out("data.text.unicode_uppercase"),
        bytes_in_out("data.text.unicode_lowercase"),
        bytes_in_out("data.text.ascii_uppercase"),
        bytes_in_out("data.text.ascii_lowercase"),
    ]
}

/// Options for Gate C runs (pilot monorepo vs full external suite).
#[derive(Debug, Clone, Default)]
pub struct GateCOptions {
    /// Mark report as external-crates campaign.
    pub external_crates: bool,
    /// Measured human review minutes (if None, use heuristic cases×3).
    pub human_minutes: Option<f64>,
    /// Optional explicit expectations (else pilot monorepo set).
    pub expectations: Option<Vec<GateCCaseExpectation>>,
    /// Gate C v2: never feed `expected_capability` into matcher/draft selection
    /// (grading remains post-hoc against the label).
    pub blind_mapping: bool,
    /// Gate C v3: do not leak `package_rel` into the matcher; use rustdoc instead.
    pub neutral_package_ids: bool,
}

/// Load expectations JSON from `fixtures/gate-c-external/expectations.json` shape.
pub fn load_gate_c_expectations_file(
    path: impl AsRef<Path>,
) -> Result<Vec<GateCCaseExpectation>, ForgeError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|e| ForgeError::Io(path.to_path_buf(), e))?;
    #[derive(Deserialize)]
    struct File {
        cases: Vec<GateCCaseExpectation>,
    }
    let f: File = serde_json::from_str(&text)
        .map_err(|e| ForgeError::Parse(path.to_path_buf(), e.to_string()))?;
    Ok(f.cases)
}

/// Full Gate C on **external** package tree (`fixtures/gate-c-external` layout).
///
/// Requires ≥5 packages, multi-domain capabilities, and records human-minutes
/// (measured via `human_minutes` or heuristic).
pub fn run_gate_c_external(
    external_root: impl AsRef<Path>,
    ontology: Option<&[OntologyCapability]>,
    run_compile: bool,
    human_minutes: Option<f64>,
) -> Result<GateCReport, ForgeError> {
    run_gate_c_external_ex(
        external_root,
        ontology,
        run_compile,
        human_minutes,
        false,
        false,
    )
}

/// Gate C v2 external campaign: **blind** mapping (≥10 cases preferred).
pub fn run_gate_c_external_v2(
    external_root: impl AsRef<Path>,
    ontology: Option<&[OntologyCapability]>,
    run_compile: bool,
    human_minutes: Option<f64>,
) -> Result<GateCReport, ForgeError> {
    run_gate_c_external_ex(
        external_root,
        ontology,
        run_compile,
        human_minutes,
        true,
        false,
    )
}

/// Gate C v3: held-out crates, **neutral package IDs**, blind mapping.
///
/// Matcher sees rustdoc + signature, never the folder name. Forge success is
/// native export + profile (when a profile exists), not cargo check alone.
pub fn run_gate_c_external_v3(
    external_root: impl AsRef<Path>,
    ontology: Option<&[OntologyCapability]>,
    run_compile: bool,
    human_minutes: Option<f64>,
) -> Result<GateCReport, ForgeError> {
    run_gate_c_external_ex(
        external_root,
        ontology,
        run_compile,
        human_minutes,
        true,
        true,
    )
}

fn run_gate_c_external_ex(
    external_root: impl AsRef<Path>,
    ontology: Option<&[OntologyCapability]>,
    run_compile: bool,
    human_minutes: Option<f64>,
    blind: bool,
    neutral: bool,
) -> Result<GateCReport, ForgeError> {
    let root = external_root.as_ref();
    let exp_path = if neutral && root.join("expectations.v3.json").is_file() {
        root.join("expectations.v3.json")
    } else if blind && root.join("expectations.v2.json").is_file() {
        root.join("expectations.v2.json")
    } else {
        root.join("expectations.json")
    };
    let expectations = if exp_path.is_file() {
        load_gate_c_expectations_file(&exp_path)?
    } else {
        return Err(ForgeError::Missing(exp_path));
    };
    let min = if neutral {
        6
    } else if blind {
        10
    } else {
        5
    };
    if expectations.len() < min {
        return Err(ForgeError::Parse(
            root.to_path_buf(),
            format!(
                "Gate C {} needs ≥{min} external cases, got {}",
                if neutral {
                    "v3 held-out"
                } else if blind {
                    "v2 blind"
                } else {
                    "external"
                },
                expectations.len()
            ),
        ));
    }
    run_gate_c(
        root,
        ontology,
        run_compile,
        GateCOptions {
            external_crates: true,
            human_minutes,
            expectations: Some(expectations),
            blind_mapping: blind,
            neutral_package_ids: neutral,
        },
    )
}

/// Run Gate C pilot metrics against `workspace_root` (weavatrix-loom monorepo).
pub fn run_gate_c_pilot(
    workspace_root: impl AsRef<Path>,
    ontology: Option<&[OntologyCapability]>,
    run_compile: bool,
) -> Result<GateCReport, ForgeError> {
    run_gate_c(
        workspace_root,
        ontology,
        run_compile,
        GateCOptions {
            external_crates: false,
            human_minutes: None,
            expectations: None,
            blind_mapping: false,
            neutral_package_ids: false,
        },
    )
}

/// Shared Gate C engine.
pub fn run_gate_c(
    workspace_root: impl AsRef<Path>,
    ontology: Option<&[OntologyCapability]>,
    run_compile: bool,
    opts: GateCOptions,
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
    let expectations = opts
        .expectations
        .clone()
        .unwrap_or_else(pilot_gate_c_expectations);
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

        // v2 blind: never use expected_capability as a hint.
        // v3: never leak package_rel; rustdoc from source is allowed.
        let docs = if opts.neutral_package_ids {
            source_docs(&pkg)
        } else {
            String::new()
        };
        let match_blob = |c: &crate::extract::ApiCandidate| -> (String, String) {
            if opts.neutral_package_ids {
                let name = format!("{} {}", c.name, docs);
                let sig = format!("{} {}", c.signature, docs);
                (name, sig)
            } else {
                let name = format!("{} {} {}", c.name, exp.package_rel, c.path);
                let sig = format!("{} {}", c.signature, exp.package_rel);
                (name, sig)
            }
        };
        let mut best = fn_candidates[0];
        let (n0, s0) = match_blob(best);
        let mut best_map = match_candidate(&n0, &s0, &best.shape, ontology);
        for c in &fn_candidates[1..] {
            let (n, s) = match_blob(c);
            let m = match_candidate(&n, &s, &c.shape, ontology);
            if !opts.blind_mapping
                && !exp.expected_capability.is_empty()
                && m.capability_key == exp.expected_capability
                && m.kind.reuses_existing()
            {
                best = c;
                best_map = m;
                break;
            }
            if kind_rank(m.kind) > kind_rank(best_map.kind)
                || (kind_rank(m.kind) == kind_rank(best_map.kind) && m.score > best_map.score)
            {
                best = c;
                best_map = m;
            }
        }
        if opts.blind_mapping {
            notes.push("blind_mapping=true (expected_capability not fed to matcher)".into());
        }

        // Post-hoc grading only (label may be empty for open-ended cases).
        let mapping_correct = if exp.expected_capability.is_empty() {
            !best_map.kind.reuses_existing() || !best_map.capability_key.starts_with("data.json.")
        } else {
            best_map.capability_key == exp.expected_capability && best_map.kind.reuses_existing()
        };

        if !mapping_correct && best_map.kind.reuses_existing() {
            notes.push("false semantic mapping risk".into());
        }

        let mut compile = None;
        // Compile when we reuse an existing capability (blind or graded).
        let should_compile = run_compile
            && exp.expect_compile
            && best_map.kind.reuses_existing()
            && (opts.blind_mapping || mapping_correct);
        if should_compile {
            let draft_report = draft_adapters_with_ontology(&pkg, Some(&exp.fn_name), ontology)?;
            // Blind: take draft for the chosen mapping capability, not the label.
            let draft = if opts.blind_mapping {
                draft_report
                    .drafts
                    .iter()
                    .find(|d| {
                        d.candidate_name == exp.fn_name
                            && d.capability_id == best_map.capability_key
                    })
                    .or_else(|| {
                        draft_report
                            .drafts
                            .iter()
                            .find(|d| d.candidate_name == exp.fn_name)
                    })
                    .or_else(|| draft_report.drafts.first())
            } else {
                draft_report
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
                    .or_else(|| draft_report.drafts.first())
            };
            if let Some(d) = draft {
                let out = compile_root.join(sanitize(&d.implementation_id));
                match compile_adapter_from_draft(&pkg, &draft_report.package_name, d, &out, true) {
                    Ok(r) => {
                        notes.push(format!("compile status={}", r.status));
                        if r.notes.iter().any(|n| n.contains("wired_upstream=true")) {
                            notes.push("runtime_wired=true".into());
                        }
                        if r.compile_ok == Some(false)
                            && r.notes.iter().any(|n| n.contains("not_wired"))
                        {
                            notes.push("not_wired rejected as compile success".into());
                        }
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
            c.found && !c.mapping_correct && c.mapping_kind.as_deref() == Some("exact_shape")
        })
        .count();

    // Pilot Go: high mapping accuracy + majority compile on expected compile cases.
    let pilot_go = extraction_recall >= 0.8
        && mapping_accuracy >= 0.8
        && (compile_cases.is_empty() || compile_rate >= 0.5)
        && false_semantic_mappings == 0;

    let _ = std::fs::remove_dir_all(&compile_root);

    // Prefer measured human minutes; else heuristic cases×3 (document as estimate).
    let human_minutes_estimate = opts.human_minutes.unwrap_or((cases.len() as f64) * 3.0);
    let measured = opts.human_minutes.is_some();

    // Full external Gate C also requires multi-domain sample (JSON + non-JSON).
    let multi_domain = cases.iter().any(|c| {
        c.expected_capability.starts_with("data.hash.")
            || c.expected_capability.starts_with("data.compress.")
            || c.expected_capability.starts_with("data.codec.")
    }) && cases
        .iter()
        .any(|c| c.expected_capability.starts_with("data.json."));

    let domains_hit = {
        let mut d = 0usize;
        if cases.iter().any(|c| {
            c.mapped_capability
                .as_deref()
                .unwrap_or("")
                .starts_with("data.json.")
                || c.expected_capability.starts_with("data.json.")
        }) {
            d += 1;
        }
        if cases.iter().any(|c| {
            let k = c
                .mapped_capability
                .as_deref()
                .unwrap_or(c.expected_capability.as_str());
            k.starts_with("data.hash.")
        }) {
            d += 1;
        }
        if cases.iter().any(|c| {
            let k = c
                .mapped_capability
                .as_deref()
                .unwrap_or(c.expected_capability.as_str());
            k.starts_with("data.compress.")
        }) {
            d += 1;
        }
        if cases.iter().any(|c| {
            let k = c
                .mapped_capability
                .as_deref()
                .unwrap_or(c.expected_capability.as_str());
            k.starts_with("data.codec.") || k.starts_with("data.text.")
        }) {
            d += 1;
        }
        d
    };

    let min_cases = if opts.neutral_package_ids {
        6
    } else if opts.blind_mapping {
        10
    } else {
        5
    };
    let min_domains = if opts.blind_mapping { 3 } else { 2 };
    let full_external_go = opts.external_crates
        && cases.len() >= min_cases
        && multi_domain
        && domains_hit >= min_domains
        && pilot_go
        && measured
        && human_minutes_estimate > 0.0
        && (!opts.blind_mapping || compile_rate >= 0.7);

    let mut notes = vec![
        if opts.external_crates {
            if opts.neutral_package_ids {
                "Gate C v3 HELD-OUT — neutral package IDs; rustdoc allowed; package_rel not fed to matcher.".into()
            } else if opts.blind_mapping {
                "Gate C v2 EXTERNAL BLIND — expected_capability not fed to matcher.".into()
            } else {
                "Gate C EXTERNAL campaign — packages outside Loom product crates/.".into()
            }
        } else {
            "Gate C pilot harness — monorepo fixtures; not full external Go.".into()
        },
        "AI/heuristic mapping never sets evidence pass (ADR-0010).".into(),
        "not_wired() adapters never count as compile success.".into(),
        format!(
            "thresholds: extraction_recall≥0.8 mapping_accuracy≥0.8 compile_rate≥0.5 false_map=0"
        ),
        format!(
            "human_minutes={} ({})",
            human_minutes_estimate,
            if measured {
                "measured/override"
            } else {
                "heuristic cases×3"
            }
        ),
        format!(
            "external_crates={} blind={} multi_domain={} domains_hit={} cases={} full_external_go={}",
            opts.external_crates,
            opts.blind_mapping,
            multi_domain,
            domains_hit,
            cases.len(),
            full_external_go
        ),
    ];
    if opts.external_crates && !measured {
        notes.push("Pass --human-minutes <N> with measured review time for full Gate C Go.".into());
    }
    if full_external_go {
        notes.push(
            if opts.neutral_package_ids {
                "Full Gate C v3 held-out criteria met (neutral IDs + native/profile forge success)."
                    .into()
            } else if opts.blind_mapping {
                "Full Gate C v2 blind criteria met (metrics + multi-domain + wired compile + human-minutes)."
                    .into()
            } else {
                "Full Gate C external criteria met (metrics + multi-domain + measured human-minutes)."
                    .into()
            },
        );
    }

    Ok(GateCReport {
        cases,
        extraction_recall,
        mapping_accuracy,
        compile_rate,
        false_semantic_mappings,
        // For external campaign, pilot_go alone is insufficient — expose combined flag in notes;
        // keep pilot_go as metric pass; CLI will use full_external when external.
        pilot_go: if opts.external_crates {
            full_external_go
        } else {
            pilot_go
        },
        notes,
        human_minutes_estimate,
        external_crates: opts.external_crates,
    })
}

fn source_docs(pkg: &Path) -> String {
    let text = std::fs::read_to_string(pkg.join("src/lib.rs")).unwrap_or_default();
    text.lines()
        .filter(|l| l.trim_start().starts_with("///"))
        .map(|l| l.trim_start().trim_start_matches('/').trim())
        .collect::<Vec<_>>()
        .join(" ")
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
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
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
                .map(|r| (
                    &r.implementation_id,
                    r.compile_ok,
                    r.compile_log
                        .as_ref()
                        .map(|s| s.chars().take(200).collect::<String>())
                ))
                .collect::<Vec<_>>()
        );
        assert!(report.pilot_go, "pilot_go false: {:?}", report.notes);
    }

    #[test]
    fn gate_c_v3_heldout_neutral_ids() {
        let root = default_workspace_root().join("fixtures/gate-c-heldout");
        let report = run_gate_c_external_v3(&root, None, true, Some(30.0)).unwrap();
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
                    &c.package_rel,
                    &c.fn_name,
                    c.mapping_correct,
                    &c.mapped_capability,
                    &c.expected_capability
                ))
                .collect::<Vec<_>>()
        );
        assert_eq!(report.false_semantic_mappings, 0);
        assert!(
            report
                .notes
                .iter()
                .any(|n| n.contains("v3") || n.contains("HELD-OUT")),
            "notes={:?}",
            report.notes
        );
        assert!(
            report.compile_rate >= 0.5,
            "compile_rate={} (forge success, not cargo check alone)",
            report.compile_rate
        );
    }
}
