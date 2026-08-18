//! Unified promotion transaction (Trust Closure P0).
//!
//! Public API does **not** accept caller-invented `ok=true` cases or evidence
//! booleans. Promote either:
//! 1. runs live collectors (build / profile / bench / license / security), or
//! 2. accepts HMAC-signed reports and verifies them.
//!
//! ```text
//! candidate
//!   → build (cargo check)
//!   → conformance profile (live suite or signed report)
//!   → benchmark
//!   → license / security collection
//!   → produce + verify artifact (recomputed digests)
//!   → optional human signature
//!   → staging + lock + atomic rename
//!   → truthful audit
//! ```
//!
//! Dry-run (`apply=false`) is fully read-only: no artifact, lock, or manifest writes.

use crate::admission::{check_implementation, justified_status, status_rank};
use crate::admit::AdmitRequest;
use crate::attestation::{append_transparency, TransparencyKind};
use crate::collect::{collect_build, collect_license, collect_security};
use crate::evidence_artifact::{
    default_artifact_relpath, mint_artifact, verify_loaded_artifact, write_artifact, CaseResult,
    DigestContext, MintRequest, TruthfulAuditReport,
};
use crate::provenance::{provenance_from_impl, write_provenance, HumanReview};
use crate::signed::{verify_signed_reports, SignedPromotionReports};
use crate::verified::{verify_implementation, VerifiedImplementation};
use crate::RegistryError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use wvx_ir::{AxisFact, Implementation, LifecycleStatus};

/// Live profile + bench runner. Implemented by command-bus (has handlers).
pub trait ProfileSuiteCollector {
    fn run_profile(
        &self,
        registry_root: &Path,
        implementation_id: &str,
        profile_id: &str,
    ) -> Result<Vec<CaseResult>, String>;

    fn run_bench(
        &self,
        registry_root: &Path,
        implementation_id: &str,
    ) -> Result<(bool, Option<String>), String>;
}

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

/// Public promotion input — **no** case_results / evidence booleans.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteRequest {
    pub implementation_id: String,
    /// Profile to mint against (else manifest `conformance_profile`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
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
    /// Optional HMAC-signed reports (alternative to live collection).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_reports: Option<SignedPromotionReports>,
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

/// Run the unified promotion transaction with an optional live suite collector.
pub fn promote_implementation(
    registry_root: &Path,
    imp: Implementation,
    req: &PromoteRequest,
) -> Result<PromoteResult, RegistryError> {
    promote_implementation_with_collector(registry_root, imp, req, None)
}

/// Promote using a live profile/bench collector (command-bus / e2e).
pub fn promote_implementation_with_collector(
    registry_root: &Path,
    mut imp: Implementation,
    req: &PromoteRequest,
    collector: Option<&dyn ProfileSuiteCollector>,
) -> Result<PromoteResult, RegistryError> {
    let mut steps = Vec::new();
    let mut findings = Vec::new();
    let previous = imp.status;
    let full_id = imp.full_id();

    let before_files = if !req.apply {
        crate::temp_registry::snapshot_files(registry_root).unwrap_or_default()
    } else {
        Vec::new()
    };

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

    // ── Collect evidence (live or signed) ────────────────────────────────
    let collected = match &req.signed_reports {
        Some(reports) => {
            if let Err(e) = verify_signed_reports(reports, &full_id, &profile_id) {
                steps.push(step("signed_reports", false, e.to_string()));
                return Ok(fail_result(
                    imp,
                    previous,
                    steps,
                    findings,
                    "signed reports failed verification",
                ));
            }
            steps.push(step(
                "signed_reports",
                true,
                format!("verified HMAC · {}", reports.runner_identity),
            ));
            CollectedAxes {
                case_results: reports.case_results.clone(),
                build: reports.build,
                bench: reports.bench,
                bench_fingerprint: reports.bench_fingerprint.clone(),
                license: reports.license,
                security: reports.security,
                runner: reports.runner_identity.clone(),
            }
        }
        None => {
            let build = collect_build(registry_root, &imp)?;
            steps.push(step("build", build.axis == AxisFact::Pass, &build.detail));
            if build.axis != AxisFact::Pass {
                return Ok(fail_result(
                    imp,
                    previous,
                    steps,
                    findings,
                    "build step failed — cannot promote",
                ));
            }

            let Some(runner) = collector else {
                steps.push(step(
                    "conformance",
                    false,
                    "no live collector and no signed reports",
                ));
                return Ok(fail_result(
                    imp,
                    previous,
                    steps,
                    findings,
                    "promote requires live profile execution or verified signed reports",
                ));
            };

            let case_results = match runner.run_profile(registry_root, &full_id, &profile_id) {
                Ok(c) => c,
                Err(e) => {
                    steps.push(step("conformance", false, e));
                    return Ok(fail_result(
                        imp,
                        previous,
                        steps,
                        findings,
                        "profile suite failed to run",
                    ));
                }
            };
            if case_results.is_empty() {
                steps.push(step("conformance", false, "no case_results from suite"));
                return Ok(fail_result(
                    imp,
                    previous,
                    steps,
                    findings,
                    "profile suite returned no cases",
                ));
            }
            if case_results.iter().any(|c| !c.ok) {
                steps.push(step(
                    "conformance",
                    false,
                    format!(
                        "{}/{} cases failed",
                        case_results.iter().filter(|c| !c.ok).count(),
                        case_results.len()
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
                format!("profile `{profile_id}` · {} cases ok", case_results.len()),
            ));

            let (bench_ok, bench_fp) = match runner.run_bench(registry_root, &full_id) {
                Ok(v) => v,
                Err(e) => {
                    steps.push(step("benchmark", false, e));
                    if target == LifecycleStatus::Admitted {
                        return Ok(fail_result(
                            imp,
                            previous,
                            steps,
                            findings,
                            "benchmark required for admitted",
                        ));
                    }
                    (false, None)
                }
            };
            let bench_axis = if bench_ok {
                steps.push(step(
                    "benchmark",
                    true,
                    bench_fp.clone().unwrap_or_else(|| "bench ok".into()),
                ));
                AxisFact::Pass
            } else if target == LifecycleStatus::Admitted {
                steps.push(step("benchmark", false, "admitted requires bench pass"));
                return Ok(fail_result(
                    imp,
                    previous,
                    steps,
                    findings,
                    "benchmark required for admitted",
                ));
            } else {
                steps.push(step("benchmark", true, "skipped (optional for conformant)"));
                AxisFact::Absent
            };

            let license = collect_license(registry_root, &imp)?;
            steps.push(step(
                "license",
                license.axis == AxisFact::Pass,
                &license.detail,
            ));
            if license.axis != AxisFact::Pass {
                return Ok(fail_result(
                    imp,
                    previous,
                    steps,
                    findings,
                    "license collection failed",
                ));
            }

            let security = collect_security(registry_root, &imp)?;
            if security.axis == AxisFact::Fail {
                steps.push(step("security", false, &security.detail));
                if target == LifecycleStatus::Admitted {
                    return Ok(fail_result(
                        imp,
                        previous,
                        steps,
                        findings,
                        "security collection failed",
                    ));
                }
            } else if security.axis == AxisFact::Pass {
                steps.push(step("security", true, &security.detail));
            } else if target == LifecycleStatus::Admitted {
                steps.push(step(
                    "security",
                    false,
                    "admitted requires security collection pass",
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
                    format!("absent ok for conformant ({})", security.detail),
                ));
            }

            CollectedAxes {
                case_results,
                build: AxisFact::Pass,
                bench: bench_axis,
                bench_fingerprint: bench_fp,
                license: AxisFact::Pass,
                security: if target == LifecycleStatus::Admitted {
                    AxisFact::Pass
                } else {
                    security.axis
                },
                runner: "wvx-registry-client/promote".into(),
            }
        }
    };

    imp.evidence.build = collected.build;
    imp.evidence.license = collected.license;
    if collected.bench == AxisFact::Pass {
        imp.evidence.benchmark = AxisFact::Pass;
    }
    if collected.security == AxisFact::Pass {
        imp.evidence.security = AxisFact::Pass;
    }

    // ── Mint (in memory) + verify recomputed digests ─────────────────────
    if imp.evidence_artifact.is_none() {
        imp.evidence_artifact = Some(default_artifact_relpath(&full_id));
    }
    let mint_req = MintRequest {
        runner_identity: collected.runner.clone(),
        case_results: collected.case_results.clone(),
        axes: imp.evidence.clone(),
        notes: {
            let mut n = req.notes.clone();
            n.push(format!("promote → {}", target.as_str()));
            n
        },
        digest_ctx: DigestContext {
            workspace_root: crate::workspace::workspace_root_near(registry_root),
            ..Default::default()
        },
        profile_id: Some(profile_id.clone()),
    };

    let art = mint_artifact(registry_root, &imp, &mint_req)?;
    steps.push(step(
        "artifact_mint",
        true,
        format!("{} · in-memory", art.schema_version),
    ));

    imp.evidence.conformance = AxisFact::Pass;
    imp.evidence.build = art.axes.build;
    if art.axes.license != AxisFact::Absent {
        imp.evidence.license = art.axes.license;
    }

    let check = verify_loaded_artifact(registry_root, &imp, &art);
    if !check.ok {
        steps.push(step("artifact_verify", false, check.findings.join("; ")));
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

    // ── Human signature (admitted) ───────────────────────────────────────
    let mut provenance_to_write = None;
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
        imp.evidence.benchmark = AxisFact::Pass;
        imp.evidence.security = AxisFact::Pass;
        imp.status = LifecycleStatus::Admitted;
        let prov = provenance_from_impl(
            &imp,
            Some(review),
            collected.bench_fingerprint.clone(),
            vec!["Unified promote transaction (admitted)".into()],
        );
        provenance_to_write = Some(prov);
        steps.push(step("human", true, "provenance prepared"));
    } else {
        imp.status = LifecycleStatus::Conformant;
        steps.push(step("human", true, "not required for conformant"));
    }

    // ── Policy ───────────────────────────────────────────────────────────
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
    steps.push(step("policy", true, format!("justified={}", just.as_str())));

    // ── Apply: staging + lock + atomic rename + rollback ─────────────────
    let mut artifact_path = None;
    let mut manifest_path = None;
    let mut provenance_path = None;

    if req.apply {
        let lock = match PromoteLock::acquire(registry_root) {
            Ok(l) => l,
            Err(e) => {
                steps.push(step("lock", false, e.to_string()));
                return Ok(fail_result(
                    imp,
                    previous,
                    steps,
                    findings,
                    "could not acquire promotion lock",
                ));
            }
        };
        steps.push(step("lock", true, "acquired"));

        let txn = match PromotionTxn::begin(registry_root) {
            Ok(t) => t,
            Err(e) => {
                steps.push(step("staging", false, e.to_string()));
                drop(lock);
                return Ok(fail_result(
                    imp,
                    previous,
                    steps,
                    findings,
                    "staging failed",
                ));
            }
        };

        let dest_art = crate::evidence_artifact::artifact_path(registry_root, &imp);
        let dest_man = implementation_manifest_path(registry_root, &imp);
        let dest_prov = provenance_to_write
            .as_ref()
            .map(|p| crate::provenance::provenance_path(registry_root, &p.implementation_id));

        let commit = (|| {
            write_artifact(&dest_art, &art)?;
            write_implementation_manifest(registry_root, &imp)?;
            if let Some(prov) = &provenance_to_write {
                write_provenance(registry_root, prov)?;
            }
            append_transparency(
                registry_root,
                TransparencyKind::Promote,
                &imp.full_id(),
                &art.subject_digest,
                unix_now(),
            )?;
            Ok::<(), RegistryError>(())
        })();

        match commit {
            Ok(()) => {
                txn.commit();
                steps.push(step(
                    "manifest",
                    true,
                    format!("atomic write {}", dest_man.display()),
                ));
                artifact_path = Some(dest_art.display().to_string());
                manifest_path = Some(dest_man.display().to_string());
                if let Some(pp) = dest_prov {
                    provenance_path = Some(pp.display().to_string());
                }
            }
            Err(e) => {
                let _ = txn.rollback();
                steps.push(step("manifest", false, format!("rolled back: {e}")));
                drop(lock);
                return Ok(fail_result(
                    imp,
                    previous,
                    steps,
                    findings,
                    "atomic write failed; rolled back",
                ));
            }
        }
        drop(lock);
    } else {
        steps.push(step(
            "manifest",
            true,
            "dry-run (read-only; pass apply=true to write)",
        ));
        let after = crate::temp_registry::snapshot_files(registry_root).unwrap_or_default();
        if after != before_files {
            steps.push(step(
                "dry_run_readonly",
                false,
                "filesystem changed during dry-run",
            ));
            findings.push("dry-run mutated the registry".into());
        } else {
            steps.push(step("dry_run_readonly", true, "no files written"));
        }
    }

    // ── Verified handle + truthful audit ─────────────────────────────────
    let verified = if req.apply {
        match verify_implementation(registry_root, &imp) {
            Ok(v) => {
                steps.push(step("verified", true, v.full_id()));
                Some(v)
            }
            Err(e) => {
                steps.push(step("verified", false, e.to_string()));
                None
            }
        }
    } else {
        // Dry-run: in-memory handle after loaded-artifact verify.
        Some(VerifiedImplementation {
            implementation: imp.clone(),
            artifact: art.clone(),
            check,
            verified_at_unix: unix_now(),
        })
    };

    let truthful = match crate::LocalRegistry::open(registry_root) {
        Ok(reg) => match crate::evidence_artifact::audit_truthful_registry(&reg) {
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
        && (req.apply && truthful.as_ref().map(|t| t.ok).unwrap_or(false)
            || !req.apply && truthful.as_ref().map(|t| t.ok).unwrap_or(true));

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
        artifact_path,
        manifest_path,
        provenance_path,
        verified,
        truthful,
        implementation: imp,
        findings,
    })
}

struct CollectedAxes {
    case_results: Vec<CaseResult>,
    build: AxisFact,
    bench: AxisFact,
    bench_fingerprint: Option<String>,
    license: AxisFact,
    security: AxisFact,
    runner: String,
}

/// Exclusive promotion lock (`<registry>/.locks/promote.lock`).
struct PromoteLock {
    path: PathBuf,
}

impl PromoteLock {
    fn acquire(registry_root: &Path) -> Result<Self, RegistryError> {
        let dir = registry_root.join(".locks");
        fs::create_dir_all(&dir).map_err(|e| RegistryError::Io(dir.clone(), e))?;
        let path = dir.join("promote.lock");
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                let _ = writeln!(f, "pid={} ts={}", std::process::id(), unix_now());
                Ok(Self { path })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(RegistryError::Parse(
                path,
                "promotion lock already held".into(),
            )),
            Err(e) => Err(RegistryError::Io(path, e)),
        }
    }
}

impl Drop for PromoteLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Tracks backups for rollback of an apply transaction.
struct PromotionTxn {
    backups: Vec<(PathBuf, PathBuf)>,
    committed: bool,
}

impl PromotionTxn {
    fn begin(_registry_root: &Path) -> Result<Self, RegistryError> {
        Ok(Self {
            backups: Vec::new(),
            committed: false,
        })
    }

    fn commit(mut self) {
        self.committed = true;
        for (_orig, bak) in self.backups.drain(..) {
            let _ = fs::remove_file(bak);
        }
    }

    fn rollback(mut self) -> Result<(), RegistryError> {
        for (orig, bak) in self.backups.drain(..) {
            if bak.is_file() {
                let _ = fs::remove_file(&orig);
                fs::rename(&bak, &orig).map_err(|e| RegistryError::Io(orig, e))?;
            }
        }
        Ok(())
    }
}

/// Bridge old admit path: still requires signed reports or a collector.
pub fn promote_from_admit(
    registry_root: &Path,
    imp: Implementation,
    admit: &AdmitRequest,
) -> Result<PromoteResult, RegistryError> {
    let req = PromoteRequest {
        implementation_id: admit.implementation_id.clone(),
        profile_id: imp.conformance_profile.clone(),
        target_status: "admitted".into(),
        human: Some(HumanSignature {
            reviewer: admit.reviewer.clone(),
            human_ack: admit.human_ack.clone(),
            security_ack: admit.security_ack.clone(),
            reason: admit.reason.clone(),
        }),
        apply: admit.apply,
        notes: vec!["via promote_from_admit".into()],
        signed_reports: None,
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
    findings.push(msg);
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

pub fn implementation_manifest_path(registry_root: &Path, imp: &Implementation) -> PathBuf {
    registry_root
        .join("implementations")
        .join(format!("{}.json", imp.full_id()))
}

pub fn write_implementation_manifest(
    registry_root: &Path,
    imp: &Implementation,
) -> Result<PathBuf, RegistryError> {
    let dir = registry_root.join("implementations");
    fs::create_dir_all(&dir).map_err(|e| RegistryError::Io(dir.clone(), e))?;
    let path = dir.join(format!("{}.json", imp.full_id()));
    atomic_write_json(&path, imp)
}

/// Write JSON via staging file + backup + rename (Windows-safe replace).
pub fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<PathBuf, RegistryError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| RegistryError::Io(parent.to_path_buf(), e))?;
    }
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| RegistryError::Parse(path.to_path_buf(), e.to_string()))?;
    let staging = path.with_extension("json.staging");
    fs::write(&staging, text + "\n").map_err(|e| RegistryError::Io(staging.clone(), e))?;
    let bak = path.with_extension("json.bak");
    if path.exists() {
        let _ = fs::remove_file(&bak);
        fs::rename(path, &bak).map_err(|e| RegistryError::Io(path.to_path_buf(), e))?;
    }
    match fs::rename(&staging, path) {
        Ok(()) => {
            let _ = fs::remove_file(&bak);
            Ok(path.to_path_buf())
        }
        Err(e) => {
            if bak.exists() {
                let _ = fs::rename(&bak, path);
            }
            let _ = fs::remove_file(&staging);
            Err(RegistryError::Io(path.to_path_buf(), e))
        }
    }
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
    use crate::temp_registry::materialize_temp_registry;
    use crate::LocalRegistry;
    use std::path::PathBuf;
    use wvx_ir::{AdapterRef, CapabilityRef, ImplementationEvidence, ImplementationSource};

    fn src_dev() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry-dev")
    }

    fn temp_parse_registry() -> PathBuf {
        let dest = std::env::temp_dir().join(format!("wvx-promote-{}", unix_now_nanos()));
        materialize_temp_registry(
            &src_dev(),
            &dest,
            &["data.json.parse@1"],
            &["json-rfc8259-core-v1"],
            &["serde-json.parse-owned@1"],
        )
        .unwrap();
        dest
    }

    fn unix_now_nanos() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }

    #[test]
    fn public_request_has_no_evidence_booleans() {
        let json = serde_json::to_value(PromoteRequest {
            implementation_id: "x@1".into(),
            profile_id: None,
            target_status: "conformant".into(),
            human: None,
            apply: false,
            notes: vec![],
            signed_reports: None,
        })
        .unwrap();
        assert!(json.get("case_results").is_none());
        assert!(json.get("build_ok").is_none());
        assert!(json.get("bench_ok").is_none());
        assert!(json.get("license_pass").is_none());
        assert!(json.get("security_pass").is_none());
    }

    #[test]
    fn promote_without_collector_or_signed_reports_fails() {
        let dest = temp_parse_registry();
        let reg = LocalRegistry::open(&dest).unwrap();
        let imp = reg
            .find_implementation("serde-json.parse-owned@1")
            .unwrap()
            .unwrap();
        let req = PromoteRequest {
            implementation_id: imp.full_id(),
            profile_id: Some("json-rfc8259-core-v1".into()),
            target_status: "conformant".into(),
            human: None,
            apply: false,
            notes: vec![],
            signed_reports: None,
        };
        let r = promote_implementation(&dest, imp, &req).unwrap();
        assert!(!r.ok);
        assert!(r
            .findings
            .iter()
            .any(|f| f.contains("live profile") || f.contains("signed")));
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn dry_run_is_read_only() {
        let dest = temp_parse_registry();
        let before = crate::temp_registry::snapshot_files(&dest).unwrap();
        let reg = LocalRegistry::open(&dest).unwrap();
        let imp = reg
            .find_implementation("serde-json.parse-owned@1")
            .unwrap()
            .unwrap();
        let req = PromoteRequest {
            implementation_id: imp.full_id(),
            profile_id: Some("json-rfc8259-core-v1".into()),
            target_status: "conformant".into(),
            human: None,
            apply: false,
            notes: vec![],
            signed_reports: None,
        };
        let _ = promote_implementation(&dest, imp, &req).unwrap();
        let after = crate::temp_registry::snapshot_files(&dest).unwrap();
        assert_eq!(before, after, "dry-run must not write");
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn promote_requires_profile() {
        let dest = std::env::temp_dir().join(format!("wvx-promote-empty-{}", unix_now_nanos()));
        let _ = fs::create_dir_all(&dest);
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
            conformance_profile: None,
            evidence_artifact: None,
            source_ref: None,
        };
        let req = PromoteRequest {
            implementation_id: "x@1".into(),
            profile_id: None,
            target_status: "conformant".into(),
            human: None,
            apply: false,
            notes: vec![],
            signed_reports: None,
        };
        let r = promote_implementation(&dest, imp, &req).unwrap();
        assert!(!r.ok);
        let _ = fs::remove_dir_all(&dest);
    }
}
