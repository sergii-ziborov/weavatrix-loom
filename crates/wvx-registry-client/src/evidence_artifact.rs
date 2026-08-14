//! Evidence artifacts — source of truth for lifecycle axes (Milestone 1).
//!
//! Manifest `evidence` axis strings are **hints only**. For `conformant` /
//! `admitted`, an on-disk artifact must exist, match subject/profile digests,
//! and report passing required suites.

use crate::{LocalRegistry, RegistryError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use wvx_ir::{AxisFact, Implementation, ImplementationEvidence, LifecycleStatus};

pub const EVIDENCE_SCHEMA: &str = "wvx.evidence.v0.1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuiteResult {
    pub profile: String,
    pub suite_digest: String,
    pub passed: bool,
    pub cases_ok: u32,
    pub cases_total: u32,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceArtifact {
    pub schema_version: String,
    pub implementation_id: String,
    pub capability_key: String,
    pub conformance_profile: String,
    /// Digest of adapter subject (emit template + package identity).
    pub subject_digest: String,
    #[serde(default)]
    pub profile_suite_digest: String,
    pub suite_results: Vec<SuiteResult>,
    pub axes: ImplementationEvidence,
    pub recorded_at_unix: u64,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactCheck {
    pub full_id: String,
    pub ok: bool,
    pub findings: Vec<String>,
    pub justified: String,
}

/// Stable subject digest from implementation identity + emit template.
///
/// Uses FNV-1a 64 (not `std::hash::DefaultHasher`, which is not cross-version stable).
pub fn subject_digest(imp: &Implementation) -> String {
    let template = imp
        .sdk
        .as_ref()
        .and_then(|s| s.emit.as_ref())
        .map(|e| e.template.as_str())
        .unwrap_or("");
    let crate_name = imp
        .sdk
        .as_ref()
        .and_then(|s| s.emit.as_ref())
        .map(|e| e.crate_name.as_str())
        .unwrap_or("");
    let payload = format!(
        "v1|{}|{}@{}|{}|{}|{}|{}",
        imp.full_id(),
        imp.capability.id,
        imp.capability.version,
        imp.source.package,
        imp.source.package_version,
        crate_name,
        template
    );
    format!("fnv1a64:{:016x}", fnv1a64(payload.as_bytes()))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut h = FNV_OFFSET;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

pub fn default_artifact_relpath(full_id: &str) -> String {
    let safe = full_id.replace(['/', '\\', ':'], "_");
    format!("evidence/artifacts/{safe}.json")
}

pub fn artifact_path(registry_root: &Path, imp: &Implementation) -> PathBuf {
    let rel = imp
        .evidence_artifact
        .clone()
        .unwrap_or_else(|| default_artifact_relpath(&imp.full_id()));
    registry_root.join(rel)
}

pub fn load_artifact(path: &Path) -> Result<EvidenceArtifact, RegistryError> {
    let text = fs::read_to_string(path).map_err(|e| RegistryError::Io(path.to_path_buf(), e))?;
    serde_json::from_str(&text).map_err(|e| RegistryError::Parse(path.to_path_buf(), e.to_string()))
}

pub fn write_artifact(path: &Path, art: &EvidenceArtifact) -> Result<(), RegistryError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| RegistryError::Io(parent.to_path_buf(), e))?;
    }
    let text = serde_json::to_string_pretty(art)
        .map_err(|e| RegistryError::Parse(path.to_path_buf(), e.to_string()))?;
    fs::write(path, text + "\n").map_err(|e| RegistryError::Io(path.to_path_buf(), e))?;
    Ok(())
}

/// Verify artifact for an implementation (Milestone 1 truthful registry).
pub fn verify_artifact(
    registry_root: &Path,
    imp: &Implementation,
) -> ArtifactCheck {
    let full_id = imp.full_id();
    let mut findings = Vec::new();
    let path = artifact_path(registry_root, imp);

    if !path.is_file() {
        findings.push(format!("missing evidence artifact at {}", path.display()));
        return ArtifactCheck {
            full_id,
            ok: false,
            findings,
            justified: LifecycleStatus::Candidate.as_str().into(),
        };
    }

    let art = match load_artifact(&path) {
        Ok(a) => a,
        Err(e) => {
            findings.push(format!("artifact unreadable: {e}"));
            return ArtifactCheck {
                full_id,
                ok: false,
                findings,
                justified: LifecycleStatus::InventoryOnly.as_str().into(),
            };
        }
    };

    if art.schema_version != EVIDENCE_SCHEMA {
        findings.push(format!(
            "artifact schema_version `{}` != `{EVIDENCE_SCHEMA}`",
            art.schema_version
        ));
    }
    if art.implementation_id != full_id {
        findings.push(format!(
            "artifact subject id `{}` != manifest `{full_id}`",
            art.implementation_id
        ));
    }
    let cap_key = format!("{}@{}", imp.capability.id, imp.capability.version);
    if art.capability_key != cap_key {
        findings.push(format!(
            "artifact capability `{}` != manifest `{cap_key}`",
            art.capability_key
        ));
    }
    if let Some(profile) = &imp.conformance_profile {
        if &art.conformance_profile != profile {
            findings.push(format!(
                "artifact profile `{}` != manifest `{profile}`",
                art.conformance_profile
            ));
        }
    } else {
        findings.push("manifest missing conformance_profile".into());
    }

    let expected_subject = subject_digest(imp);
    if art.subject_digest != expected_subject {
        findings.push(format!(
            "subject digest mismatch: artifact `{}` vs computed `{expected_subject}`",
            art.subject_digest
        ));
    }

    let all_suites_pass = !art.suite_results.is_empty()
        && art.suite_results.iter().all(|s| s.passed && s.cases_ok == s.cases_total);
    if art.suite_results.is_empty() {
        findings.push("artifact has no suite_results".into());
    } else if !all_suites_pass {
        findings.push("one or more suite_results failed".into());
    }

    // Axes for justified status come from **artifact**, not free-form manifest.
    let mut axes = art.axes.clone();
    if all_suites_pass && axes.conformance != AxisFact::Fail {
        axes.conformance = AxisFact::Pass;
    }
    if !all_suites_pass {
        axes.conformance = AxisFact::Fail;
    }

    let justified = justified_from_axes(imp, &axes, all_suites_pass && findings.is_empty());
    let ok = findings.is_empty()
        && all_suites_pass
        && crate::admission::status_rank(imp.status)
            <= crate::admission::status_rank(parse_status(&justified));

    ArtifactCheck {
        full_id,
        ok,
        findings,
        justified,
    }
}

fn parse_status(s: &str) -> LifecycleStatus {
    match s {
        "admitted" => LifecycleStatus::Admitted,
        "conformant" => LifecycleStatus::Conformant,
        "candidate" => LifecycleStatus::Candidate,
        _ => LifecycleStatus::InventoryOnly,
    }
}

fn justified_from_axes(
    imp: &Implementation,
    e: &ImplementationEvidence,
    suites_ok: bool,
) -> String {
    let has_adapter = imp.adapter.is_some();
    let any_fail = [e.build, e.conformance, e.benchmark, e.license, e.security]
        .iter()
        .any(|a| *a == AxisFact::Fail);
    if any_fail || !suites_ok {
        return if has_adapter {
            LifecycleStatus::Candidate.as_str().into()
        } else {
            LifecycleStatus::InventoryOnly.as_str().into()
        };
    }
    if has_adapter
        && e.conformance == AxisFact::Pass
        && e.build == AxisFact::Pass
        && e.license == AxisFact::Pass
        && e.security == AxisFact::Pass
        && e.benchmark == AxisFact::Pass
    {
        return LifecycleStatus::Admitted.as_str().into();
    }
    if has_adapter && e.conformance == AxisFact::Pass && e.build != AxisFact::Fail {
        return LifecycleStatus::Conformant.as_str().into();
    }
    if has_adapter {
        LifecycleStatus::Candidate.as_str().into()
    } else {
        LifecycleStatus::InventoryOnly.as_str().into()
    }
}

/// Full truthful audit: every impl, artifact rules for status ≥ conformant.
pub fn audit_truthful_registry(reg: &LocalRegistry) -> Result<TruthfulAuditReport, RegistryError> {
    let impls = reg.list_implementations()?;
    let mut items = Vec::new();
    let mut overclaims = 0usize;
    let mut missing_artifacts = 0usize;

    for imp in &impls {
        let declared = imp.status;
        let needs_artifact = matches!(
            declared,
            LifecycleStatus::Conformant | LifecycleStatus::Admitted
        );

        let (justified, findings, artifact_ok) = if needs_artifact {
            let check = verify_artifact(reg.root(), imp);
            if !check.findings.is_empty() {
                missing_artifacts += 1;
            }
            let j = parse_status(&check.justified);
            (j, check.findings, check.ok)
        } else {
            // Candidate / inventory: still prefer artifact when present.
            let path = artifact_path(reg.root(), imp);
            if path.is_file() {
                let check = verify_artifact(reg.root(), imp);
                (parse_status(&check.justified), check.findings, check.ok)
            } else {
                (
                    crate::admission::justified_status(imp),
                    vec!["no artifact (ok for candidate/inventory_only)".into()],
                    true,
                )
            }
        };

        let overclaim =
            crate::admission::status_rank(declared) > crate::admission::status_rank(justified);
        if overclaim {
            overclaims += 1;
        }

        items.push(TruthfulAuditItem {
            full_id: imp.full_id(),
            declared: declared.as_str().into(),
            justified: justified.as_str().into(),
            overclaim,
            artifact_ok,
            needs_artifact,
            findings,
        });
    }

    let ok = overclaims == 0
        && items
            .iter()
            .filter(|i| i.needs_artifact)
            .all(|i| i.artifact_ok && !i.overclaim);

    Ok(TruthfulAuditReport {
        ok,
        checked: items.len(),
        overclaims,
        missing_or_bad_artifacts: missing_artifacts,
        items,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruthfulAuditItem {
    pub full_id: String,
    pub declared: String,
    pub justified: String,
    pub overclaim: bool,
    pub artifact_ok: bool,
    pub needs_artifact: bool,
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruthfulAuditReport {
    pub ok: bool,
    pub checked: usize,
    pub overclaims: usize,
    pub missing_or_bad_artifacts: usize,
    pub items: Vec<TruthfulAuditItem>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::{AxisFact, LifecycleStatus};

    #[test]
    fn mint_and_verify_serde_json_parse_artifact() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry-dev");
        let reg = LocalRegistry::open(&root).expect("registry-dev");
        let mut imp = reg
            .find_implementation("serde-json.parse-owned@1")
            .unwrap()
            .expect("impl");
        let digest = subject_digest(&imp);
        let rel = "evidence/artifacts/serde-json.parse-owned@1.json";
        let path = root.join(rel);
        let art = EvidenceArtifact {
            schema_version: EVIDENCE_SCHEMA.into(),
            implementation_id: imp.full_id(),
            capability_key: format!("{}@{}", imp.capability.id, imp.capability.version),
            conformance_profile: "json-rfc8259-core-v1".into(),
            subject_digest: digest.clone(),
            profile_suite_digest: "sha256:pilot-suite-placeholder".into(),
            suite_results: vec![SuiteResult {
                profile: "json-rfc8259-core-v1".into(),
                suite_digest: "sha256:pilot-suite-placeholder".into(),
                passed: true,
                cases_ok: 8,
                cases_total: 8,
                notes: vec!["Gate A pilot vectors".into()],
            }],
            axes: ImplementationEvidence {
                build: AxisFact::Pass,
                conformance: AxisFact::Pass,
                benchmark: AxisFact::Absent,
                license: AxisFact::Pass,
                security: AxisFact::Absent,
            },
            recorded_at_unix: 1_700_000_000,
            notes: vec!["Milestone 1 truthful registry sample".into()],
        };
        write_artifact(&path, &art).unwrap();
        imp.status = LifecycleStatus::Conformant;
        imp.conformance_profile = Some("json-rfc8259-core-v1".into());
        imp.evidence_artifact = Some(rel.into());
        imp.evidence.conformance = AxisFact::Pass;
        let out = root.join("implementations/serde-json.parse-owned@1.json");
        fs::write(&out, serde_json::to_string_pretty(&imp).unwrap() + "\n").unwrap();

        let check = verify_artifact(&root, &imp);
        assert!(check.ok, "{:?}", check.findings);
        assert_eq!(check.justified, "conformant");

        let audit = audit_truthful_registry(&reg).unwrap();
        assert!(
            audit.ok,
            "truthful audit failed: overclaims={} bad_art={}",
            audit.overclaims, audit.missing_or_bad_artifacts
        );
    }
}
