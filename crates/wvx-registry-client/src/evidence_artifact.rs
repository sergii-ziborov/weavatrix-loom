//! Evidence artifacts — source of truth for lifecycle axes.
//!
//! - **v0.1** (`wvx.evidence.v0.1`): subject digest + suite summary (legacy).
//! - **v0.2** (`wvx.evidence.v0.2`): full trust digests, environment, case-by-case
//!   results; verifier loads the conformance profile and rechecks linkages.
//!
//! Manifest `evidence` axis strings are **hints only**. For `conformant` /
//! `admitted`, an on-disk artifact must exist and pass verification.

use crate::{LocalRegistry, RegistryError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use wvx_ir::{AxisFact, Implementation, ImplementationEvidence, LifecycleStatus};

/// Current mint schema.
pub const EVIDENCE_SCHEMA: &str = "wvx.evidence.v0.2";
/// Legacy schema still accepted by the verifier.
pub const EVIDENCE_SCHEMA_V01: &str = "wvx.evidence.v0.1";

// ─── Artifact model ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SuiteResult {
    pub profile: String,
    pub suite_digest: String,
    pub passed: bool,
    pub cases_ok: u32,
    pub cases_total: u32,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Single vector / negative outcome recorded by a conformance runner.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CaseResult {
    pub case_id: String,
    /// `positive` | `negative`
    #[serde(default = "default_case_kind")]
    pub kind: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error_family: Option<String>,
}

fn default_case_kind() -> String {
    "positive".into()
}

/// Cryptographic / content digests that pin the subject of evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EvidenceDigests {
    /// SHA-256 of implementation identity + source pins (logical source tree).
    #[serde(default)]
    pub implementation_source_tree: String,
    /// SHA-256 of upstream package identity (`kind|package@version`).
    #[serde(default)]
    pub upstream_package: String,
    /// SHA-256 of workspace/export `Cargo.lock` when present, else `sha256:absent`.
    #[serde(default)]
    pub cargo_lock: String,
    /// SHA-256 of adapter emit template + crate binding.
    #[serde(default)]
    pub adapter_source: String,
    /// SHA-256 of the capability contract JSON on disk.
    #[serde(default)]
    pub capability_contract: String,
    /// SHA-256 of the conformance profile document on disk.
    #[serde(default)]
    pub profile: String,
    /// SHA-256 of profile vectors (+ negatives), or profile.suite_digest when set.
    #[serde(default)]
    pub suite: String,
    /// Legacy-compatible subject pin (emit + package).
    #[serde(default)]
    pub subject: String,
    /// SHA-256 of adapter crate `Cargo.toml` (or package identity fallback).
    #[serde(default)]
    pub package_checksum: String,
    /// SHA-256 of `source_ref.revision`, or `sha256:absent`.
    #[serde(default)]
    pub source_ref_revision: String,
    /// SHA-256 of exact profile case IDs (sorted `kind:id`).
    #[serde(default)]
    pub profile_case_ids: String,
}

/// Where / who produced the artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EvidenceEnvironment {
    /// Rust target triple (e.g. `x86_64-pc-windows-msvc`) or `unknown`.
    #[serde(default)]
    pub target: String,
    /// `rustc` version string when available.
    #[serde(default)]
    pub toolchain: String,
    /// Feature flags active for the run (if any).
    #[serde(default)]
    pub features: Vec<String>,
    /// Runner identity: host + crate (e.g. `wvx-conformance@0.1.1`).
    #[serde(default)]
    pub runner_identity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceArtifact {
    pub schema_version: String,
    pub implementation_id: String,
    pub capability_key: String,
    pub conformance_profile: String,
    /// Digest of adapter subject (v0.1 + v0.2 mirror of digests.subject).
    pub subject_digest: String,
    #[serde(default)]
    pub profile_suite_digest: String,
    pub suite_results: Vec<SuiteResult>,
    pub axes: ImplementationEvidence,
    pub recorded_at_unix: u64,
    #[serde(default)]
    pub notes: Vec<String>,
    // ─── v0.2 fields (default empty when loading v0.1) ────────────────────
    #[serde(default)]
    pub digests: EvidenceDigests,
    #[serde(default)]
    pub environment: EvidenceEnvironment,
    #[serde(default)]
    pub case_results: Vec<CaseResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactCheck {
    pub full_id: String,
    pub ok: bool,
    pub findings: Vec<String>,
    pub justified: String,
    /// Schema of the artifact that was verified (`v0.1` / `v0.2` / missing).
    #[serde(default)]
    pub schema_version: String,
}

// ─── Digest helpers ───────────────────────────────────────────────────────────

/// Hex SHA-256 with `sha256:` prefix.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let hash = Sha256::digest(bytes);
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

/// Stable subject digest from implementation identity + emit template.
///
/// v0.2 stores this as `digests.subject` and mirrors into `subject_digest`.
/// Also keeps FNV form as legacy suffix-compatible check for v0.1 artifacts.
pub fn subject_digest(imp: &Implementation) -> String {
    subject_digest_sha256(imp)
}

/// Legacy FNV-1a 64 used by v0.1 sample artifacts.
pub fn subject_digest_fnv(imp: &Implementation) -> String {
    let payload = subject_payload(imp);
    format!("fnv1a64:{:016x}", fnv1a64(payload.as_bytes()))
}

pub fn subject_digest_sha256(imp: &Implementation) -> String {
    sha256_hex(subject_payload(imp).as_bytes())
}

fn subject_payload(imp: &Implementation) -> String {
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
    format!(
        "v1|{}|{}@{}|{}|{}|{}|{}",
        imp.full_id(),
        imp.capability.id,
        imp.capability.version,
        imp.source.package,
        imp.source.package_version,
        crate_name,
        template
    )
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

// ─── Profile load ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceProfileDoc {
    pub id: String,
    #[serde(default)]
    pub version: String,
    pub capability_key: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub suite_digest: String,
    #[serde(default)]
    pub vectors: Vec<serde_json::Value>,
    #[serde(default)]
    pub negative_vectors: Vec<serde_json::Value>,
    #[serde(default)]
    pub expected_error_families: Vec<String>,
    #[serde(default)]
    pub guarantees: Vec<String>,
    #[serde(default)]
    pub limitations: Vec<String>,
}

pub fn profile_path(registry_root: &Path, profile_id: &str) -> PathBuf {
    registry_root
        .join("profiles")
        .join(format!("{profile_id}.json"))
}

pub fn load_profile(
    registry_root: &Path,
    profile_id: &str,
) -> Result<(ConformanceProfileDoc, Vec<u8>), RegistryError> {
    let path = profile_path(registry_root, profile_id);
    let bytes = fs::read(&path).map_err(|e| RegistryError::Io(path.clone(), e))?;
    let doc: ConformanceProfileDoc =
        serde_json::from_slice(&bytes).map_err(|e| RegistryError::Parse(path, e.to_string()))?;
    Ok((doc, bytes))
}

/// Exact profile case IDs as `positive:<id>` / `negative:<id>` (sorted).
pub fn profile_case_ids(doc: &ConformanceProfileDoc) -> Vec<String> {
    let mut ids = Vec::new();
    for vec in &doc.vectors {
        let id = vec.get("id").and_then(|v| v.as_str()).unwrap_or("unnamed");
        ids.push(format!("positive:{id}"));
    }
    for vec in &doc.negative_vectors {
        let id = vec.get("id").and_then(|v| v.as_str()).unwrap_or("neg");
        ids.push(format!("negative:{id}"));
    }
    ids.sort();
    ids.dedup();
    ids
}

/// Digest of exact profile case IDs.
pub fn profile_case_ids_digest(doc: &ConformanceProfileDoc) -> String {
    sha256_hex(profile_case_ids(doc).join("\n").as_bytes())
}

/// Suite digest: use profile field unless placeholder; else hash vectors.
pub fn suite_digest_for_profile(doc: &ConformanceProfileDoc) -> String {
    let d = doc.suite_digest.trim();
    if !d.is_empty()
        && !d.contains("pending")
        && d.starts_with("sha256:")
        && d.len() > "sha256:".len() + 8
    {
        return d.to_string();
    }
    let payload = serde_json::json!({
        "id": doc.id,
        "capability_key": doc.capability_key,
        "vectors": doc.vectors,
        "negative_vectors": doc.negative_vectors,
    });
    sha256_hex(payload.to_string().as_bytes())
}

// ─── Compute digests for an impl ──────────────────────────────────────────────

/// Options for minting / recomputing digests.
#[derive(Debug, Clone, Default)]
pub struct DigestContext {
    /// Absolute path to capability JSON (optional).
    pub capability_path: Option<PathBuf>,
    /// Absolute path to profile JSON (optional).
    pub profile_path: Option<PathBuf>,
    /// Absolute path to Cargo.lock (optional).
    pub cargo_lock_path: Option<PathBuf>,
    /// Absolute path to adapter source file or tree root (optional).
    pub adapter_source_path: Option<PathBuf>,
    /// Absolute path to implementation source tree (optional).
    pub source_tree_path: Option<PathBuf>,
    /// Workspace root (Cargo.lock + adapter crates). Auto-discovered when absent.
    pub workspace_root: Option<PathBuf>,
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(sha256_hex(&bytes))
}

fn path_digest_or_absent(path: Option<&Path>) -> String {
    match path {
        Some(p) if p.is_file() => file_sha256(p).unwrap_or_else(|_| "sha256:absent".into()),
        Some(p) if p.is_dir() => hash_tree(p).unwrap_or_else(|_| "sha256:absent".into()),
        _ => "sha256:absent".into(),
    }
}

pub fn hash_tree(root: &Path) -> Result<String, String> {
    let mut entries: Vec<PathBuf> = Vec::new();
    walk_collect(root, &mut entries)?;
    entries.sort();
    let mut hasher = Sha256::new();
    for path in entries {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        let bytes = fs::read(&path).map_err(|e| format!("{rel}: {e}"))?;
        hasher.update(&bytes);
        hasher.update([0]);
    }
    let hash = hasher.finalize();
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!("sha256:{hex}"))
}

fn walk_collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let rd = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in rd {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name == "target" || name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk_collect(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

/// Compute full digest set for an implementation under a registry.
pub fn compute_digests(
    registry_root: &Path,
    imp: &Implementation,
    profile: Option<&ConformanceProfileDoc>,
    ctx: &DigestContext,
) -> EvidenceDigests {
    let subject = subject_digest_sha256(imp);
    let upstream = sha256_hex(
        format!(
            "{}|{}@{}",
            imp.source.kind, imp.source.package, imp.source.package_version
        )
        .as_bytes(),
    );

    let workspace = ctx
        .workspace_root
        .clone()
        .or_else(|| crate::workspace::workspace_root_near(registry_root));

    let adapter = if let Some(ws) = &workspace {
        crate::workspace::adapter_source_closure_digest(ws, imp)
    } else {
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
        let crate_path = imp
            .sdk
            .as_ref()
            .and_then(|s| s.emit.as_ref())
            .and_then(|e| e.crate_path.as_deref())
            .unwrap_or("");
        sha256_hex(format!("{crate_name}|{crate_path}|{template}").as_bytes())
    };

    let cap_key = format!("{}@{}", imp.capability.id, imp.capability.version);
    let cap_path = ctx.capability_path.clone().unwrap_or_else(|| {
        registry_root
            .join("capabilities")
            .join(format!("{cap_key}.json"))
    });
    let capability_contract = path_digest_or_absent(Some(&cap_path));

    let profile_id = imp
        .conformance_profile
        .clone()
        .unwrap_or_else(|| profile.map(|p| p.id.clone()).unwrap_or_default());
    let prof_path = ctx
        .profile_path
        .clone()
        .unwrap_or_else(|| profile_path(registry_root, &profile_id));
    let profile_digest = path_digest_or_absent(Some(&prof_path));

    let (suite, case_ids_digest) = if let Some(p) = profile {
        (suite_digest_for_profile(p), profile_case_ids_digest(p))
    } else if let Ok((doc, _)) = load_profile(registry_root, &profile_id) {
        (
            suite_digest_for_profile(&doc),
            profile_case_ids_digest(&doc),
        )
    } else {
        ("sha256:absent".into(), "sha256:absent".into())
    };

    let cargo_lock = if let Some(p) = &ctx.cargo_lock_path {
        path_digest_or_absent(Some(p))
    } else if let Some(ws) = &workspace {
        crate::workspace::cargo_lock_digest(ws)
    } else {
        let guess = registry_root.join("../Cargo.lock");
        path_digest_or_absent(Some(&guess))
    };

    let source_tree = if let Some(p) = &ctx.source_tree_path {
        path_digest_or_absent(Some(p))
    } else if let Some(p) = &ctx.adapter_source_path {
        path_digest_or_absent(Some(p))
    } else if let Some(ws) = &workspace {
        crate::workspace::implementation_source_tree_digest(ws, imp)
    } else {
        sha256_hex(format!("tree|{subject}|{adapter}|{upstream}").as_bytes())
    };

    let pkg = if let Some(ws) = &workspace {
        crate::workspace::package_checksum(ws, imp)
    } else {
        sha256_hex(
            format!(
                "pkg|{}|{}@{}",
                imp.source.kind, imp.source.package, imp.source.package_version
            )
            .as_bytes(),
        )
    };
    let src_rev = crate::workspace::source_ref_revision_digest(imp);

    EvidenceDigests {
        implementation_source_tree: source_tree,
        upstream_package: upstream,
        cargo_lock,
        adapter_source: adapter,
        capability_contract,
        profile: profile_digest,
        suite,
        subject,
        package_checksum: pkg,
        source_ref_revision: src_rev,
        profile_case_ids: case_ids_digest,
    }
}

pub fn capture_environment(runner_identity: &str) -> EvidenceEnvironment {
    let target = std::env::var("TARGET")
        .or_else(|_| std::env::var("CARGO_CFG_TARGET_TRIPLE"))
        .unwrap_or_else(|_| {
            // Best-effort host triple
            format!(
                "{}-{}-{}",
                std::env::consts::ARCH,
                std::env::consts::FAMILY,
                std::env::consts::OS
            )
        });
    let toolchain = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".into());
    EvidenceEnvironment {
        target,
        toolchain,
        features: Vec::new(),
        runner_identity: runner_identity.into(),
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ─── Mint ─────────────────────────────────────────────────────────────────────

/// Input for minting a v0.2 artifact from a conformance / suite run.
#[derive(Debug, Clone, Default)]
pub struct MintRequest {
    pub runner_identity: String,
    pub case_results: Vec<CaseResult>,
    pub axes: ImplementationEvidence,
    pub notes: Vec<String>,
    pub digest_ctx: DigestContext,
    /// Override profile id (else manifest conformance_profile).
    pub profile_id: Option<String>,
}

/// Mint a **v0.2** evidence artifact (does not write until [`write_artifact`]).
pub fn mint_artifact(
    registry_root: &Path,
    imp: &Implementation,
    req: &MintRequest,
) -> Result<EvidenceArtifact, RegistryError> {
    let profile_id = req
        .profile_id
        .clone()
        .or_else(|| imp.conformance_profile.clone())
        .ok_or_else(|| {
            RegistryError::Parse(
                registry_root.to_path_buf(),
                "mint requires conformance_profile on impl or request".into(),
            )
        })?;

    let (profile_doc, _) = load_profile(registry_root, &profile_id)?;
    let cap_key = format!("{}@{}", imp.capability.id, imp.capability.version);
    if profile_doc.capability_key != cap_key {
        return Err(RegistryError::Parse(
            profile_path(registry_root, &profile_id),
            format!(
                "profile capability `{}` != impl `{cap_key}`",
                profile_doc.capability_key
            ),
        ));
    }

    let digests = compute_digests(registry_root, imp, Some(&profile_doc), &req.digest_ctx);
    let environment = capture_environment(if req.runner_identity.is_empty() {
        "wvx-registry-client"
    } else {
        &req.runner_identity
    });

    let cases_total = req.case_results.len() as u32;
    let cases_ok = req.case_results.iter().filter(|c| c.ok).count() as u32;
    let passed = cases_total > 0 && cases_ok == cases_total;

    let mut axes = req.axes.clone();
    if passed {
        if axes.conformance != AxisFact::Fail {
            axes.conformance = AxisFact::Pass;
        }
    } else if cases_total > 0 {
        axes.conformance = AxisFact::Fail;
    }
    if axes.build == AxisFact::Absent {
        axes.build = AxisFact::Pass; // local mint assumes build ok if suite ran
    }

    Ok(EvidenceArtifact {
        schema_version: EVIDENCE_SCHEMA.into(),
        implementation_id: imp.full_id(),
        capability_key: cap_key,
        conformance_profile: profile_id,
        subject_digest: digests.subject.clone(),
        profile_suite_digest: digests.suite.clone(),
        suite_results: vec![SuiteResult {
            profile: profile_doc.id.clone(),
            suite_digest: digests.suite.clone(),
            passed,
            cases_ok,
            cases_total,
            notes: vec![format!(
                "minted by {} at {}",
                environment.runner_identity, environment.target
            )],
        }],
        axes,
        recorded_at_unix: now_unix(),
        notes: req.notes.clone(),
        digests,
        environment,
        case_results: req.case_results.clone(),
    })
}

/// Convenience: mint + write under default or manifest-relative path.
pub fn mint_and_write(
    registry_root: &Path,
    imp: &Implementation,
    req: &MintRequest,
) -> Result<(EvidenceArtifact, PathBuf), RegistryError> {
    let art = mint_artifact(registry_root, imp, req)?;
    let path = artifact_path(registry_root, imp);
    write_artifact(&path, &art)?;
    Ok((art, path))
}

// ─── Verify ───────────────────────────────────────────────────────────────────

/// Verify artifact for an implementation (v0.1 legacy or v0.2 full).
///
/// For v0.2 the verifier **loads the profile** and recomputes digests.
pub fn verify_artifact(registry_root: &Path, imp: &Implementation) -> ArtifactCheck {
    let full_id = imp.full_id();
    let path = artifact_path(registry_root, imp);

    if !path.is_file() {
        return ArtifactCheck {
            full_id,
            ok: false,
            findings: vec![format!("missing evidence artifact at {}", path.display())],
            justified: LifecycleStatus::Candidate.as_str().into(),
            schema_version: String::new(),
        };
    }

    match load_artifact(&path) {
        Ok(art) => verify_loaded_artifact(registry_root, imp, &art),
        Err(e) => ArtifactCheck {
            full_id,
            ok: false,
            findings: vec![format!("artifact unreadable: {e}")],
            justified: LifecycleStatus::InventoryOnly.as_str().into(),
            schema_version: String::new(),
        },
    }
}

/// Verify an already-loaded (or in-memory dry-run) artifact.
pub fn verify_loaded_artifact(
    registry_root: &Path,
    imp: &Implementation,
    art: &EvidenceArtifact,
) -> ArtifactCheck {
    let full_id = imp.full_id();
    let mut findings = Vec::new();
    let art = art.clone();

    let schema = art.schema_version.clone();
    let is_v2 = schema == EVIDENCE_SCHEMA;
    let is_v1 = schema == EVIDENCE_SCHEMA_V01;
    if !is_v1 && !is_v2 {
        findings.push(format!(
            "unsupported artifact schema_version `{schema}` (expected `{EVIDENCE_SCHEMA}` or `{EVIDENCE_SCHEMA_V01}`)"
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

    // Subject digest
    if is_v1 {
        let expected_fnv = subject_digest_fnv(imp);
        let expected_sha = subject_digest_sha256(imp);
        if art.subject_digest != expected_fnv && art.subject_digest != expected_sha {
            findings.push(format!(
                "subject digest mismatch: artifact `{}` vs computed `{expected_fnv}` or `{expected_sha}`",
                art.subject_digest
            ));
        }
    } else {
        let expected_sha = subject_digest_sha256(imp);
        if art.subject_digest != expected_sha {
            findings.push(format!(
                "subject digest mismatch: artifact `{}` vs computed `{expected_sha}`",
                art.subject_digest
            ));
        }
        if !art.digests.subject.is_empty() && art.digests.subject != expected_sha {
            findings.push(format!(
                "digests.subject mismatch: artifact `{}` vs computed `{expected_sha}`",
                art.digests.subject
            ));
        }
    }

    // ─── v0.2: load profile + recompute digests ───────────────────────────
    if is_v2 {
        match load_profile(registry_root, &art.conformance_profile) {
            Ok((doc, _)) => {
                if doc.capability_key != cap_key {
                    findings.push(format!(
                        "profile capability `{}` != artifact/impl `{cap_key}`",
                        doc.capability_key
                    ));
                }
                if doc.id != art.conformance_profile {
                    findings.push(format!(
                        "profile document id `{}` != artifact profile `{}`",
                        doc.id, art.conformance_profile
                    ));
                }
                let expected_suite = suite_digest_for_profile(&doc);
                if !art.profile_suite_digest.is_empty()
                    && art.profile_suite_digest != expected_suite
                    && art.digests.suite != expected_suite
                {
                    // soft if profile still has pending-compute and we recompute
                    if art.digests.suite != expected_suite {
                        findings.push(format!(
                            "suite digest mismatch: artifact digests.suite `{}` vs recomputed `{expected_suite}`",
                            art.digests.suite
                        ));
                    }
                }
                for sr in &art.suite_results {
                    if sr.profile != doc.id && sr.profile != art.conformance_profile {
                        findings.push(format!(
                            "suite_results profile `{}` != artifact profile `{}`",
                            sr.profile, art.conformance_profile
                        ));
                    }
                    if !sr.suite_digest.is_empty()
                        && sr.suite_digest != expected_suite
                        && sr.suite_digest != art.digests.suite
                    {
                        findings.push(format!(
                            "suite_results.suite_digest `{}` does not match recomputed `{expected_suite}`",
                            sr.suite_digest
                        ));
                    }
                }

                let recomputed =
                    compute_digests(registry_root, imp, Some(&doc), &DigestContext::default());
                check_digest(
                    &mut findings,
                    "implementation_source_tree",
                    &art.digests.implementation_source_tree,
                    &recomputed.implementation_source_tree,
                );
                check_digest(
                    &mut findings,
                    "adapter_source",
                    &art.digests.adapter_source,
                    &recomputed.adapter_source,
                );
                check_digest(
                    &mut findings,
                    "cargo_lock",
                    &art.digests.cargo_lock,
                    &recomputed.cargo_lock,
                );
                check_digest(
                    &mut findings,
                    "package_checksum",
                    &art.digests.package_checksum,
                    &recomputed.package_checksum,
                );
                check_digest(
                    &mut findings,
                    "source_ref_revision",
                    &art.digests.source_ref_revision,
                    &recomputed.source_ref_revision,
                );
                check_digest(
                    &mut findings,
                    "profile_case_ids",
                    &art.digests.profile_case_ids,
                    &recomputed.profile_case_ids,
                );
                check_digest(
                    &mut findings,
                    "upstream_package",
                    &art.digests.upstream_package,
                    &recomputed.upstream_package,
                );
                check_digest(
                    &mut findings,
                    "capability_contract",
                    &art.digests.capability_contract,
                    &recomputed.capability_contract,
                );
                check_digest(
                    &mut findings,
                    "profile",
                    &art.digests.profile,
                    &recomputed.profile,
                );
                // Exact profile case IDs (not caller-invented v0..vN).
                let expected_ids = profile_case_ids(&doc);
                let recorded: Vec<String> = art
                    .case_results
                    .iter()
                    .map(|c| normalize_case_id(&c.case_id, &c.kind))
                    .collect();
                let mut recorded_set = recorded.clone();
                recorded_set.sort();
                recorded_set.dedup();
                if recorded_set != expected_ids {
                    findings.push(format!(
                        "exact profile case IDs mismatch: artifact {recorded_set:?} vs profile {expected_ids:?}"
                    ));
                }
                // suite: use recomputed expected_suite
                if !art.digests.suite.is_empty() && art.digests.suite != expected_suite {
                    findings.push(format!(
                        "digests.suite `{}` != recomputed suite `{expected_suite}`",
                        art.digests.suite
                    ));
                }
                if art.environment.runner_identity.trim().is_empty() {
                    findings.push("environment.runner_identity is empty".into());
                }
                if art.environment.target.trim().is_empty() {
                    findings.push("environment.target is empty".into());
                }
            }
            Err(e) => {
                findings.push(format!(
                    "cannot load profile `{}`: {e}",
                    art.conformance_profile
                ));
            }
        }

        // case_results consistency with suite_results
        if !art.case_results.is_empty() {
            let ok_n = art.case_results.iter().filter(|c| c.ok).count() as u32;
            let total = art.case_results.len() as u32;
            if let Some(sr) = art.suite_results.first() {
                if sr.cases_total != total || sr.cases_ok != ok_n {
                    findings.push(format!(
                        "case_results ({ok_n}/{total}) != suite_results ({}/{})",
                        sr.cases_ok, sr.cases_total
                    ));
                }
                let expect_pass = ok_n == total && total > 0;
                if sr.passed != expect_pass {
                    findings.push(format!(
                        "suite_results.passed={} but case_results imply passed={expect_pass}",
                        sr.passed
                    ));
                }
            }
        }
    }

    let all_suites_pass = !art.suite_results.is_empty()
        && art
            .suite_results
            .iter()
            .all(|s| s.passed && s.cases_ok == s.cases_total);
    if art.suite_results.is_empty() {
        findings.push("artifact has no suite_results".into());
    } else if !all_suites_pass {
        findings.push("one or more suite_results failed".into());
    }

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
        schema_version: schema,
    }
}

/// Strip implementation prefix from runner case ids (`impl:pos:id` → `positive:id`).
pub fn normalize_case_id(case_id: &str, kind: &str) -> String {
    let kind = if kind == "negative" {
        "negative"
    } else {
        "positive"
    };
    // Already normalized
    if let Some(rest) = case_id.strip_prefix("positive:") {
        return format!("positive:{rest}");
    }
    if let Some(rest) = case_id.strip_prefix("negative:") {
        return format!("negative:{rest}");
    }
    // Runner form: `{impl}:pos:{id}` / `{impl}:neg:{id}` / `{impl}:pos:{id}:eq`
    let parts: Vec<&str> = case_id.split(':').collect();
    if parts.len() >= 3 && (parts[1] == "pos" || parts[1] == "neg") {
        let k = if parts[1] == "neg" {
            "negative"
        } else {
            "positive"
        };
        return format!("{k}:{}", parts[2]);
    }
    format!("{kind}:{case_id}")
}

fn check_digest(findings: &mut Vec<String>, name: &str, artifact: &str, expected: &str) {
    if artifact.is_empty() {
        findings.push(format!("digests.{name} is empty"));
        return;
    }
    if artifact != expected && expected != "sha256:absent" {
        findings.push(format!(
            "digests.{name} mismatch: artifact `{artifact}` vs recomputed `{expected}`"
        ));
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

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::{AxisFact, LifecycleStatus};

    #[test]
    fn mint_v2_and_verify_serde_json_parse() {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry-dev");
        let dest = std::env::temp_dir().join(format!(
            "wvx-mint-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        crate::temp_registry::materialize_temp_registry(
            &src,
            &dest,
            &["data.json.parse@1"],
            &["json-rfc8259-core-v1"],
            &["serde-json.parse-owned@1"],
        )
        .unwrap();
        let reg = LocalRegistry::open(&dest).expect("temp registry");
        let mut imp = reg
            .find_implementation("serde-json.parse-owned@1")
            .unwrap()
            .expect("impl");
        imp.conformance_profile = Some("json-rfc8259-core-v1".into());
        imp.status = LifecycleStatus::Conformant;
        imp.evidence_artifact = Some("evidence/artifacts/serde-json.parse-owned@1.json".into());

        let (doc, _) = load_profile(&dest, "json-rfc8259-core-v1").unwrap();
        let cases: Vec<CaseResult> = profile_case_ids(&doc)
            .into_iter()
            .map(|id| {
                let kind = if id.starts_with("negative:") {
                    "negative"
                } else {
                    "positive"
                };
                CaseResult {
                    case_id: id,
                    kind: kind.into(),
                    ok: true,
                    detail: None,
                    expected_error_family: None,
                }
            })
            .collect();

        let req = MintRequest {
            runner_identity: "wvx-registry-client@test".into(),
            case_results: cases.clone(),
            axes: ImplementationEvidence {
                build: AxisFact::Pass,
                conformance: AxisFact::Pass,
                benchmark: AxisFact::Absent,
                license: AxisFact::Pass,
                security: AxisFact::Absent,
            },
            notes: vec!["EvidenceArtifact v0.2 mint test".into()],
            digest_ctx: DigestContext::default(),
            profile_id: Some("json-rfc8259-core-v1".into()),
        };

        let (art, path) = mint_and_write(&dest, &imp, &req).expect("mint");
        assert_eq!(art.schema_version, EVIDENCE_SCHEMA);
        assert!(!art.digests.subject.is_empty());
        assert!(!art.digests.profile.is_empty());
        assert!(!art.digests.suite.is_empty());
        assert!(!art.digests.profile_case_ids.is_empty());
        assert!(!art.environment.runner_identity.is_empty());
        assert_eq!(art.case_results.len(), cases.len());
        assert!(path.is_file());

        let check = verify_artifact(&dest, &imp);
        assert!(check.ok, "verify failed: {:?}", check.findings);
        assert_eq!(check.justified, "conformant");
        assert_eq!(check.schema_version, EVIDENCE_SCHEMA);
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn suite_digest_stable() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry-dev");
        let (doc, _) = load_profile(&root, "json-rfc8259-core-v1").unwrap();
        let a = suite_digest_for_profile(&doc);
        let b = suite_digest_for_profile(&doc);
        assert_eq!(a, b);
        assert!(a.starts_with("sha256:"));
    }
}
