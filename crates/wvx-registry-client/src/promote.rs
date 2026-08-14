//! Unified promotion transaction (Trust Closure P0).
//!
//! ```text
//! candidate
//!   → build
//!   → conformance profile (mint artifact)
//!   → benchmark
//!   → license / security collection
//!   → produce + verify artifact
//!   → optional human signature
//!   → atomic manifest update
//!   → truthful audit
//! ```
//!
//! Old `admit` path remains for human-only Gate E; this module is the
//! single product transaction for promotion.

use crate::admission::{check_implementation, justified_status, status_rank};
use crate::admit::AdmitRequest;
use crate::evidence_artifact::{
    audit_truthful_registry, default_artifact_relpath, mint_and_write, verify_artifact, CaseResult,
    MintRequest, TruthfulAuditReport,
};
use crate::provenance::{provenance_from_impl, write_provenance, HumanReview};
use crate::verified::{verify_implementation, VerifiedImplementation};
use crate::RegistryError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use wvx_ir::{AxisFact, Implementation, LifecycleStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteStep {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HumanSignature {
    pub reviewer: String,
    pub human_ack: String,
    pub security_ack: String,
    pub reason: String,
}

/// Input for a full promotion transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteRequest {
    pub implementation_id: String,
    /// Profile to mint against (else manifest `conformance_profile`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    /// Case results from conformance runner (required non-empty for promote).
    #[serde(default)]
    pub case_results: Vec<CaseResult>,
    /// Build axis: when true, set build=pass.
    #[serde(default = "default_true")]
    pub build_ok: bool,
    /// Benchmark axis: when true, set benchmark=pass.
    #[serde(default)]
    pub bench_ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bench_fingerprint: Option<String>,
    /// License collection result.
    #[serde(default = "default_true")]
    pub license_pass: bool,
    /// Security collection / review result (required for `admitted`).
    #[serde(default)]
    pub security_pass: bool,
    /// Target lifecycle after success: `conformant` or `admitted`.
    #[serde(default = "default_conformant")]
    pub target_status: String,
    /// Optional human signature (required when target is admitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human: Option<HumanSignature>,
    /// When true, write implementation manifest + artifact atomically.
    #[serde(default)]
    pub apply: bool,
    #[serde(default)]
    pub notes: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_conformant() -> String {
    "conformant".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteResult {
    pub ok: bool,
    pub implementation_id: String,
    pub previous_status: String,
    pub new_status: String,
    pub justified: String,
    pub steps: Vec<PromoteStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified: Option<VerifiedImplementation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truthful: Option<TruthfulAuditReport>,
    pub implementation: Implementation,
    pub findings: Vec<String>,
}

fn step(name: &str, ok: bool, detail: impl Into<String>) -> PromoteStep {
    PromoteStep {
        name: name.into(),
        ok,
        detail: detail.into(),
    }
}

/// Run the unified promotion transaction.
pub fn promote_implementation(
    registry_root: &Path,
    mut imp: Implementation,
    req: &PromoteRequest,
) -> Result<PromoteResult, RegistryError> {
    let mut steps = Vec::new();
    let mut findings = Vec::new();
    let previous = imp.status;
    let full_id = imp.full_id();

    let target = match req.target_status.as_str() {
        "admitted" => LifecycleStatus::Admitted,
        "conformant" => LifecycleStatus::Conformant,
        other => {
            return Ok(fail_result(
                imp,
                previous,
                steps,
                findings,
                format!("unknown target_status `{other}` (use conformant|admitted)"),
            ));
        }
    };

    // ── 1. Build ──────────────────────────────────────────────────────────
    if !req.build_ok {
        steps.push(step("build", false, "build_ok=false"));
        return Ok(fail_result(
            imp,
            previous,
            steps,
            findings,
            "build step failed — cannot promote",
        ));
    }
    imp.evidence.build = AxisFact::Pass;
    steps.push(step("build", true, "evidence.build=pass"));

    // ── 2. Profile / conformance ──────────────────────────────────────────
    let profile_id = req
        .profile_id
        .clone()
        .or_else(|| imp.conformance_profile.clone());
    let Some(profile_id) = profile_id else {
        steps.push(step("conformance", false, "no profile_id"));
        return Ok(fail_result(
            imp,
            previous,
            steps,
            findings,
            "conformance profile required",
        ));
    };
    imp.conformance_profile = Some(profile_id.clone());

    if req.case_results.is_empty() {
        steps.push(step("conformance", false, "no case_results"));
        return Ok(fail_result(
            imp,
            previous,
            steps,
            findings,
            "case_results required — run profile suite first",
        ));
    }
    if req.case_results.iter().any(|c| !c.ok) {
        steps.push(step(
            "conformance",
            false,
            format!(
                "{}/{} cases failed",
                req.case_results.iter().filter(|c| !c.ok).count(),
                req.case_results.len()
            ),
        ));
        return Ok(fail_result(
            imp,
            previous,
            steps,
            findings,
            "conformance suite not fully green",
        ));
    }
    steps.push(step(
        "conformance",
        true,
        format!(
            "profile `{profile_id}` · {} cases ok",
            req.case_results.len()
        ),
    ));

    // ── 3. Benchmark ──────────────────────────────────────────────────────
    if req.bench_ok {
        imp.evidence.benchmark = AxisFact::Pass;
        steps.push(step(
            "benchmark",
            true,
            req.bench_fingerprint
                .clone()
                .unwrap_or_else(|| "bench_ok".into()),
        ));
    } else if target == LifecycleStatus::Admitted {
        steps.push(step(
            "benchmark",
            false,
            "admitted requires bench_ok=true",
        ));
        return Ok(fail_result(
            imp,
            previous,
            steps,
            findings,
            "benchmark required for admitted",
        ));
    } else {
        steps.push(step(
            "benchmark",
            true,
            "skipped (optional for conformant)",
        ));
    }

    // ── 4. License / security ─────────────────────────────────────────────
    if req.license_pass {
        imp.evidence.license = AxisFact::Pass;
        steps.push(step("license", true, "evidence.license=pass"));
    } else {
        steps.push(step("license", false, "license_pass=false"));
        return Ok(fail_result(
            imp,
            previous,
            steps,
            findings,
            "license collection failed",
        ));
    }

    if req.security_pass {
        imp.evidence.security = AxisFact::Pass;
        steps.push(step("security", true, "evidence.security=pass"));
    } else if target == LifecycleStatus::Admitted {
        steps.push(step(
            "security",
            false,
            "admitted requires security_pass=true",
        ));
        return Ok(fail_result(
            imp,
            previous,
            steps,
            findings,
            "security collection required for admitted",
        ));
    } else {
        steps.push(step(
            "security",
            true,
            "absent ok for conformant (not admitted)",
        ));
    }

    if imp.adapter.is_none() {
        steps.push(step("adapter", false, "no adapter binding"));
        return Ok(fail_result(
            imp,
            previous,
            steps,
            findings,
            "adapter required for promotion",
        ));
    }
    steps.push(step("adapter", true, "adapter present"));

    // ── 5. Mint + verify artifact ─────────────────────────────────────────
    if imp.evidence_artifact.is_none() {
        imp.evidence_artifact = Some(default_artifact_relpath(&full_id));
    }
    let mint_req = MintRequest {
        runner_identity: "wvx-registry-client/promote".into(),
        case_results: req.case_results.clone(),
        axes: imp.evidence.clone(),
        notes: {
            let mut n = req.notes.clone();
            n.push(format!("promote → {}", target.as_str()));
            n
        },
        digest_ctx: Default::default(),
        profile_id: Some(profile_id.clone()),
    };

    // Always write artifact during transaction (needed for verify); dry-run still mints.
    let (art, art_path) = mint_and_write(registry_root, &imp, &mint_req)?;
    steps.push(step(
        "artifact_mint",
        true,
        format!("{} · {}", art.schema_version, art_path.display()),
    ));

    // Sync axes from artifact (conformance pass)
    imp.evidence.conformance = AxisFact::Pass;
    imp.evidence.build = art.axes.build;
    if art.axes.license != AxisFact::Absent {
        imp.evidence.license = art.axes.license;
    }

    let check = verify_artifact(registry_root, &imp);
    if !check.ok {
        steps.push(step(
            "artifact_verify",
            false,
            check.findings.join("; "),
        ));
        return Ok(fail_result(
            imp,
            previous,
            steps,
            findings,
            "artifact verification failed",
        ));
    }
    steps.push(step(
        "artifact_verify",
        true,
        format!("justified={}", check.justified),
    ));

    // ── 6. Human signature (admitted) ─────────────────────────────────────
    let mut provenance_path = None;
    if target == LifecycleStatus::Admitted {
        let Some(human) = &req.human else {
            steps.push(step("human", false, "missing human signature"));
            return Ok(fail_result(
                imp,
                previous,
                steps,
                findings,
                "human signature required for admitted",
            ));
        };
        if human.reviewer.trim().is_empty()
            || human.human_ack.trim().len() < 8
            || human.security_ack.trim().len() < 8
        {
            steps.push(step("human", false, "incomplete human acks"));
            return Ok(fail_result(
                imp,
                previous,
                steps,
                findings,
                "human/security ack too short or reviewer empty",
            ));
        }
        let review = HumanReview {
            reviewer: human.reviewer.trim().into(),
            ack: human.human_ack.trim().into(),
            security_ack: human.security_ack.trim().into(),
            reason: human.reason.trim().into(),
            reviewed_at_unix: unix_now(),
        };
        // Ensure admitted axes
        imp.evidence.benchmark = AxisFact::Pass;
        imp.evidence.security = AxisFact::Pass;
        imp.status = LifecycleStatus::Admitted;
        let prov = provenance_from_impl(
            &imp,
            Some(review),
            req.bench_fingerprint.clone(),
            vec!["Unified promote transaction (admitted)".into()],
        );
        let pp = write_provenance(registry_root, &prov)?;
        provenance_path = Some(pp.display().to_string());
        steps.push(step("human", true, format!("provenance {}", pp.display())));
    } else {
        imp.status = LifecycleStatus::Conformant;
        steps.push(step("human", true, "not required for conformant"));
    }

    // ── 7. Justified + overclaim ──────────────────────────────────────────
    let just = justified_status(&imp);
    if status_rank(imp.status) > status_rank(just) {
        steps.push(step(
            "policy",
            false,
            format!(
                "overclaim: declared {} > justified {}",
                imp.status.as_str(),
                just.as_str()
            ),
        ));
        imp.status = previous;
        return Ok(fail_result(
            imp,
            previous,
            steps,
            findings,
            "justified status below target",
        ));
    }
    let chk = check_implementation(&imp);
    if !chk.ok {
        steps.push(step(
            "policy",
            false,
            chk.findings
                .iter()
                .map(|f| f.message.clone())
                .collect::<Vec<_>>()
                .join("; "),
        ));
        imp.status = previous;
        return Ok(fail_result(
            imp,
            previous,
            steps,
            findings,
            "admission overclaim check failed",
        ));
    }
    steps.push(step(
        "policy",
        true,
        format!("justified={}", just.as_str()),
    ));

    // ── 8. Atomic manifest write ──────────────────────────────────────────
    let mut manifest_path = None;
    if req.apply {
        let path = write_implementation_manifest(registry_root, &imp)?;
        steps.push(step(
            "manifest",
            true,
            format!("wrote {}", path.display()),
        ));
        manifest_path = Some(path.display().to_string());
    } else {
        steps.push(step(
            "manifest",
            true,
            "dry-run (pass apply=true to write)",
        ));
    }

    // ── 9. Verified handle + truthful audit ───────────────────────────────
    let verified = match verify_implementation(registry_root, &imp) {
        Ok(v) => {
            steps.push(step("verified", true, v.full_id()));
            Some(v)
        }
        Err(e) => {
            steps.push(step("verified", false, e.to_string()));
            None
        }
    };

    let truthful = match crate::LocalRegistry::open(registry_root) {
        Ok(reg) => match audit_truthful_registry(&reg) {
            Ok(r) => {
                steps.push(step(
                    "truthful_audit",
                    r.ok,
                    format!(
                        "checked={} overclaims={} bad={}",
                        r.checked, r.overclaims, r.missing_or_bad_artifacts
                    ),
                ));
                Some(r)
            }
            Err(e) => {
                steps.push(step("truthful_audit", false, e.to_string()));
                None
            }
        },
        Err(e) => {
            steps.push(step("truthful_audit", false, e.to_string()));
            None
        }
    };

    let ok = steps.iter().all(|s| s.ok)
        && verified.is_some()
        && truthful.as_ref().map(|t| t.ok).unwrap_or(false);

    if !ok {
        findings.push("one or more promotion steps failed".into());
    }

    Ok(PromoteResult {
        ok,
        implementation_id: full_id,
        previous_status: previous.as_str().into(),
        new_status: imp.status.as_str().into(),
        justified: just.as_str().into(),
        steps,
        artifact_path: Some(art_path.display().to_string()),
        manifest_path,
        provenance_path,
        verified,
        truthful,
        implementation: imp,
        findings,
    })
}

/// Bridge old admit path into promote for admitted-only (compat).
pub fn promote_from_admit(
    registry_root: &Path,
    imp: Implementation,
    admit: &AdmitRequest,
    case_results: Vec<CaseResult>,
) -> Result<PromoteResult, RegistryError> {
    let req = PromoteRequest {
        implementation_id: admit.implementation_id.clone(),
        profile_id: imp.conformance_profile.clone(),
        case_results,
        build_ok: true,
        bench_ok: true,
        bench_fingerprint: Some(admit.bench_fingerprint.clone()),
        license_pass: true,
        security_pass: true,
        target_status: "admitted".into(),
        human: Some(HumanSignature {
            reviewer: admit.reviewer.clone(),
            human_ack: admit.human_ack.clone(),
            security_ack: admit.security_ack.clone(),
            reason: admit.reason.clone(),
        }),
        apply: admit.apply,
        notes: vec!["via promote_from_admit".into()],
    };
    promote_implementation(registry_root, imp, &req)
}

fn fail_result(
    imp: Implementation,
    previous: LifecycleStatus,
    steps: Vec<PromoteStep>,
    mut findings: Vec<String>,
    msg: impl Into<String>,
) -> PromoteResult {
    let msg = msg.into();
    findings.push(msg.clone());
    PromoteResult {
        ok: false,
        implementation_id: imp.full_id(),
        previous_status: previous.as_str().into(),
        new_status: previous.as_str().into(),
        justified: justified_status(&imp).as_str().into(),
        steps,
        artifact_path: None,
        manifest_path: None,
        provenance_path: None,
        verified: None,
        truthful: None,
        implementation: imp,
        findings,
    }
}

pub fn write_implementation_manifest(
    registry_root: &Path,
    imp: &Implementation,
) -> Result<PathBuf, RegistryError> {
    let dir = registry_root.join("implementations");
    fs::create_dir_all(&dir).map_err(|e| RegistryError::Io(dir.clone(), e))?;
    let path = dir.join(format!("{}.json", imp.full_id()));
    let text = serde_json::to_string_pretty(imp)
        .map_err(|e| RegistryError::Parse(path.clone(), e.to_string()))?;
    fs::write(&path, text + "\n").map_err(|e| RegistryError::Io(path.clone(), e))?;
    Ok(path)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalRegistry;
    use wvx_ir::{AdapterRef, CapabilityRef, ImplementationEvidence, ImplementationSource};

    #[test]
    fn promote_serde_json_to_conformant_dry_run() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry-dev");
        let reg = LocalRegistry::open(&root).unwrap();
        let imp = reg
            .find_implementation("serde-json.parse-owned@1")
            .unwrap()
            .unwrap();
        let cases: Vec<CaseResult> = (0..8)
            .map(|i| CaseResult {
                case_id: format!("v{i}"),
                kind: "positive".into(),
                ok: true,
                detail: None,
                expected_error_family: None,
            })
            .collect();
        let req = PromoteRequest {
            implementation_id: imp.full_id(),
            profile_id: Some("json-rfc8259-core-v1".into()),
            case_results: cases,
            build_ok: true,
            bench_ok: false,
            bench_fingerprint: None,
            license_pass: true,
            security_pass: false,
            target_status: "conformant".into(),
            human: None,
            apply: false,
            notes: vec!["unit test promote".into()],
        };
        let r = promote_implementation(&root, imp, &req).unwrap();
        assert!(r.ok, "steps={:?} findings={:?}", r.steps, r.findings);
        assert_eq!(r.new_status, "conformant");
        assert!(r.verified.is_some());
        assert!(r.truthful.as_ref().is_some_and(|t| t.ok));
    }

    #[test]
    fn promote_requires_cases() {
        let dir = std::env::temp_dir().join(format!("wvx-promote-{}", unix_now()));
        let _ = fs::create_dir_all(&dir);
        let imp = Implementation {
            id: "x".into(),
            version: "1".into(),
            capability: CapabilityRef::new("data.json.parse", "1"),
            source: ImplementationSource {
                kind: "test".into(),
                package: "x".into(),
                package_version: "1".into(),
                notes: None,
            },
            adapter: Some(AdapterRef {
                crate_name: "x".into(),
                execution: "native-rust".into(),
            }),
            status: LifecycleStatus::Candidate,
            evidence: ImplementationEvidence::default(),
            notes: None,
            sdk: None,
            conformance_profile: Some("json-rfc8259-core-v1".into()),
            evidence_artifact: None,
        };
        let req = PromoteRequest {
            implementation_id: "x@1".into(),
            profile_id: Some("json-rfc8259-core-v1".into()),
            case_results: vec![],
            build_ok: true,
            bench_ok: false,
            bench_fingerprint: None,
            license_pass: true,
            security_pass: false,
            target_status: "conformant".into(),
            human: None,
            apply: false,
            notes: vec![],
        };
        let r = promote_implementation(&dir, imp, &req).unwrap();
        assert!(!r.ok);
        let _ = fs::remove_dir_all(&dir);
    }
}
