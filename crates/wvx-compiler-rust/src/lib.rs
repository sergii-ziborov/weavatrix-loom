//! Compile a validated WVX project to a readable, cargo-buildable Rust package.
//!
//! Production adapters live in the external `wvx-adapters` crate and are
//! **vendored** into each export under `vendor/wvx-adapters` so the package is
//! self-contained.
//!
//! Milestone 2: [`CompilePolicy`] enforces release lifecycle, trusted emit subset,
//! digests, optional Cargo.lock, TargetProfile + resolution explanations.

mod adapters;
mod emit;
mod order;
mod vendor;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;
use thiserror::Error;
use wvx_ir::{
    AxisFact, Implementation, LifecycleStatus, Project, ResolveDecision, ResolverPolicy, SdkEmit,
    TargetProfile,
};
use wvx_registry_client::{resolve_implementation, VerifiedImplementation};
use wvx_validator::{validate_project, validate_project_with, ValidateOptions, ValidationReport};

pub use adapters::{
    built_in_sdk_emit as adapters_built_in_sdk_emit, default_implementation, is_passthrough_io,
    known_implementation_ids, wasm_incompatible_reason,
};

/// Optional sidecar target (ADR-0006). Native remains the default production path.
pub const WASM_TARGET: &str = "wasm32-wasip1";

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("project is not valid: {0}")]
    InvalidProject(String),
    #[error("unsupported implementation `{0}` for capability `{1}`")]
    UnsupportedImplementation(String, String),
    #[error("policy rejected implementation `{0}`: {1}")]
    PolicyRejected(String, String),
    #[error("graph error: {0}")]
    Graph(String),
    #[error("io: {0}")]
    Io(String),
    #[error("cargo failed: {0}")]
    Cargo(String),
}

/// Release / dev compile policy (Milestone 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilePolicy {
    /// Policy id for resolution explanations.
    pub id: String,
    /// When false, candidate/inventory_only implementations are rejected.
    #[serde(default = "default_true")]
    pub allow_candidate: bool,
    /// Require evidence.conformance == pass when implementation metadata is known.
    #[serde(default)]
    pub require_conformance_pass: bool,
    /// Only emit code for known/trusted implementations (catalog + sdk map).
    #[serde(default = "default_true")]
    pub trusted_emit_only: bool,
    /// Target profile for explainable resolution.
    #[serde(default)]
    pub target_profile: TargetProfile,
    /// Resolver knobs (mirrors registry ResolverPolicy).
    #[serde(default)]
    pub resolver: ResolverPolicy,
    /// Known implementations for lifecycle/policy checks and resolution.
    #[serde(default)]
    pub implementations: Vec<Implementation>,
    /// After writing export, run `cargo generate-lockfile`.
    #[serde(default)]
    pub generate_cargo_lock: bool,
    /// Attach SHA-256 digests of generated files.
    #[serde(default = "default_true")]
    pub compute_digests: bool,
}

fn default_true() -> bool {
    true
}

impl Default for CompilePolicy {
    fn default() -> Self {
        Self::dev()
    }
}

impl CompilePolicy {
    /// Dev-friendly: candidates allowed, trusted emit on.
    pub fn dev() -> Self {
        Self {
            id: "compile.dev".into(),
            allow_candidate: true,
            require_conformance_pass: false,
            trusted_emit_only: true,
            target_profile: TargetProfile {
                id: "dev".into(),
                prefer_pure_rust: true,
                ..Default::default()
            },
            resolver: ResolverPolicy {
                id: "compile.dev".into(),
                allow_candidate: true,
                require_conformance_pass: false,
                require_build_pass: false,
                require_license_pass: false,
                require_security_pass: false,
                require_verified_artifact: false,
                prefer_impl_ids: Vec::new(),
            },
            implementations: Vec::new(),
            generate_cargo_lock: false,
            compute_digests: true,
        }
    }

    /// Release: no candidates, Pass-only axes, verified artifact required.
    pub fn release() -> Self {
        Self {
            id: "compile.release".into(),
            allow_candidate: false,
            require_conformance_pass: true,
            trusted_emit_only: true,
            target_profile: TargetProfile {
                id: "release".into(),
                prefer_pure_rust: true,
                prefer_no_unsafe: true,
                ..Default::default()
            },
            resolver: ResolverPolicy::release(),
            implementations: Vec::new(),
            generate_cargo_lock: true,
            compute_digests: true,
        }
    }

    fn to_validate_options(&self) -> ValidateOptions {
        ValidateOptions {
            implementations: self.implementations.clone(),
            strict_schema: true,
            require_release_lifecycle: !self.allow_candidate,
            allowed_compiler_profiles: if self.allow_candidate {
                vec!["dev".into(), "release".into(), "check".into()]
            } else {
                vec!["release".into(), "check".into()]
            },
            require_known_implementation: !self.allow_candidate && !self.implementations.is_empty(),
        }
    }

    fn to_resolver_policy(&self) -> ResolverPolicy {
        let mut r = self.resolver.clone();
        r.allow_candidate = self.allow_candidate;
        if self.require_conformance_pass {
            r.require_conformance_pass = true;
        }
        r
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFile {
    pub relative_path: String,
    pub contents: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedWorkspace {
    pub files: Vec<GeneratedFile>,
    pub package_name: String,
}

impl GeneratedWorkspace {
    /// Write all files under `root`, creating parent directories.
    pub fn write_to(&self, root: &Path) -> Result<(), CompileError> {
        for file in &self.files {
            let path = root.join(&file.relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| CompileError::Io(e.to_string()))?;
            }
            fs::write(&path, &file.contents).map_err(|e| CompileError::Io(e.to_string()))?;
        }
        Ok(())
    }
}

/// Full compile report with digests and resolution explanations (M2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileReport {
    pub workspace: GeneratedWorkspace,
    pub validation: ValidationReport,
    /// Relative path → sha256 hex.
    #[serde(default)]
    pub digests: BTreeMap<String, String>,
    /// Per-capability (or per-instance) resolution decisions.
    #[serde(default)]
    pub resolutions: Vec<ResolveDecision>,
    /// instance_id → implementation_id
    #[serde(default)]
    pub resolved_implementations: BTreeMap<String, String>,
    pub policy_id: String,
    pub profile_id: String,
}

/// Generate a Cargo package for the project (in memory).
pub fn compile_to_rust(project: &Project) -> Result<GeneratedWorkspace, CompileError> {
    Ok(compile_with_policy(project, &BTreeMap::new(), &CompilePolicy::dev())?.workspace)
}

/// Like [`compile_to_rust`], with Gate F SDK emit map (`implementation_id` → emit).
pub fn compile_to_rust_with_sdk(
    project: &Project,
    sdk_emits: &BTreeMap<String, SdkEmit>,
) -> Result<GeneratedWorkspace, CompileError> {
    Ok(compile_with_policy(project, sdk_emits, &CompilePolicy::dev())?.workspace)
}

/// Release compile: **only** accepts [`VerifiedImplementation`] pool.
///
/// Fail-closed when `verified` is empty or an instance selects an unverified impl
/// (I/O passthrough excepted).
pub fn compile_release(
    project: &Project,
    verified: &[VerifiedImplementation],
    sdk_emits: &BTreeMap<String, SdkEmit>,
) -> Result<CompileReport, CompileError> {
    if verified.is_empty() {
        return Err(CompileError::PolicyRejected(
            "(pool)".into(),
            "compile_release requires non-empty VerifiedImplementation pool (not raw manifests)"
                .into(),
        ));
    }
    let mut policy = CompilePolicy::release();
    policy.implementations = verified.iter().map(|v| v.implementation.clone()).collect();
    // I/O passthrough is not VerifiedImplementation; inject explicit I/O ids
    // so release validation does not treat them as unknown.
    for inst in &project.instances {
        let cap = inst.capability.as_key();
        if !adapters::is_passthrough_io(&cap) {
            continue;
        }
        let id = inst
            .implementation
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| default_implementation(&cap).map(str::to_string));
        let Some(id) = id else {
            continue;
        };
        if policy.implementations.iter().any(|i| i.full_id() == id) {
            continue;
        }
        policy.implementations.push(Implementation {
            id: id.rsplit_once('@').map(|(a, _)| a).unwrap_or(&id).into(),
            version: id.rsplit_once('@').map(|(_, v)| v).unwrap_or("1").into(),
            capability: inst.capability.clone(),
            source: Default::default(),
            adapter: None,
            status: LifecycleStatus::Conformant,
            evidence: wvx_ir::ImplementationEvidence {
                build: AxisFact::Pass,
                conformance: AxisFact::Pass,
                license: AxisFact::Pass,
                ..Default::default()
            },
            notes: Some("passthrough I/O".into()),
            sdk: None,
            conformance_profile: None,
            evidence_artifact: None,
            source_ref: None,
        });
    }
    // Ensure every non-IO instance maps to a verified id
    let verified_ids: BTreeSet<String> = verified.iter().map(|v| v.full_id()).collect();
    for inst in &project.instances {
        let cap = inst.capability.as_key();
        if adapters::is_passthrough_io(&cap) {
            continue;
        }
        let chosen = inst
            .implementation
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| default_implementation(&cap).map(str::to_string));
        if let Some(id) = chosen {
            if !verified_ids.contains(&id) {
                return Err(CompileError::PolicyRejected(
                    id,
                    format!(
                        "instance `{}` implementation is not in VerifiedImplementation pool",
                        inst.id
                    ),
                ));
            }
        } else {
            return Err(CompileError::PolicyRejected(
                "(none)".into(),
                format!("instance `{}` has no implementation for release", inst.id),
            ));
        }
    }
    compile_with_policy(project, sdk_emits, &policy)
}

/// Write a [`compile_release`] workspace to disk, optionally `cargo check` / `cargo run`.
pub fn export_release_to_directory(
    project: &Project,
    verified: &[VerifiedImplementation],
    out_dir: &Path,
    check: bool,
    run_input: Option<&[u8]>,
    sdk_emits: &BTreeMap<String, SdkEmit>,
) -> Result<ExportReport, CompileError> {
    let report = compile_release(project, verified, sdk_emits)?;
    let policy = CompilePolicy::release();
    write_compiled_workspace(&report, out_dir, &policy, check, run_input)
}

/// Compile with explicit policy (release/dev), digests, and resolution explanations.
pub fn compile_with_policy(
    project: &Project,
    sdk_emits: &BTreeMap<String, SdkEmit>,
    policy: &CompilePolicy,
) -> Result<CompileReport, CompileError> {
    let vopts = policy.to_validate_options();
    let report = validate_project_with(project, &vopts);
    if !report.is_ok() {
        let msg = report
            .errors()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(CompileError::InvalidProject(msg));
    }

    let order = order::topo_order(project).map_err(CompileError::Graph)?;
    let (resolved, resolutions) = resolve_implementations_explained(project, sdk_emits, policy)?;
    let needed_impls: BTreeSet<String> = resolved.values().cloned().collect();

    // Trusted emit subset
    if policy.trusted_emit_only {
        let trusted: BTreeSet<&str> = known_implementation_ids().into_iter().collect();
        for impl_id in &needed_impls {
            let cap = project
                .instances
                .iter()
                .find(|i| resolved.get(&i.id) == Some(impl_id))
                .map(|i| i.capability.as_key())
                .unwrap_or_default();
            if adapters::is_passthrough_io(&cap) {
                continue;
            }
            let has_sdk = sdk_emits.contains_key(impl_id)
                || adapters::built_in_sdk_emit(impl_id).is_some()
                || adapters::crate_module(impl_id).is_some();
            if !trusted.contains(impl_id.as_str()) && !sdk_emits.contains_key(impl_id) {
                if !has_sdk {
                    return Err(CompileError::PolicyRejected(
                        impl_id.clone(),
                        "not in trusted emit catalog and no sdk.emit provided".into(),
                    ));
                }
            }
            if !has_sdk {
                return Err(CompileError::PolicyRejected(
                    impl_id.clone(),
                    "no trusted emitter for implementation".into(),
                ));
            }
        }
    }

    let pkg = sanitize_pkg_name(&project.id);
    let mut files = Vec::new();

    files.push(GeneratedFile {
        relative_path: "Cargo.toml".into(),
        contents: emit::cargo_toml(&pkg, &needed_impls, sdk_emits),
    });

    files.push(GeneratedFile {
        relative_path: "weavatrix.lock".into(),
        contents: emit::lockfile_with_meta(project, &resolved, policy, &resolutions),
    });

    let mut vendored_crates = BTreeSet::new();
    if adapters::needs_external_adapters(&needed_impls) {
        files.extend(vendor::vendor_adapters_files()?);
        vendored_crates.insert("wvx-adapters".to_string());
    }
    for (impl_id, sdk) in sdk_emits {
        if !needed_impls.contains(impl_id) {
            continue;
        }
        if vendored_crates.contains(&sdk.crate_name) {
            continue;
        }
        if let Some(src) = sdk.crate_path.as_ref() {
            files.extend(vendor::vendor_crate_files(src, &sdk.crate_name)?);
            vendored_crates.insert(sdk.crate_name.clone());
        }
    }

    files.push(GeneratedFile {
        relative_path: "src/generated_pipeline.rs".into(),
        contents: emit::pipeline(project, &order, &resolved, sdk_emits)?,
    });

    files.push(GeneratedFile {
        relative_path: "src/main.rs".into(),
        contents: emit::main_rs(),
    });

    files.push(GeneratedFile {
        relative_path: "src/lib.rs".into(),
        contents: "//! Generated Loom export.\n//!\n//! Uses vendored adapters under `vendor/`.\n\npub mod generated_pipeline;\n\npub use generated_pipeline::{run_pipeline, run_pipeline_named, run_pipeline_read};\n".into(),
    });

    let workspace = GeneratedWorkspace {
        files,
        package_name: pkg,
    };

    let digests = if policy.compute_digests {
        digest_workspace(&workspace)
    } else {
        BTreeMap::new()
    };

    Ok(CompileReport {
        workspace,
        validation: report,
        digests,
        resolutions,
        resolved_implementations: resolved,
        policy_id: policy.id.clone(),
        profile_id: policy.target_profile.id.clone(),
    })
}

fn digest_workspace(ws: &GeneratedWorkspace) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for f in &ws.files {
        // Skip large vendor trees from digest map noise? Still include for integrity.
        let hash = Sha256::digest(f.contents.as_bytes());
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        m.insert(f.relative_path.clone(), hex);
    }
    m
}

/// Compile, write to `out_dir`, optionally `cargo check` / `cargo run`.
pub fn export_to_directory(
    project: &Project,
    out_dir: &Path,
    check: bool,
    run_input: Option<&[u8]>,
) -> Result<ExportReport, CompileError> {
    export_to_directory_with_sdk(project, out_dir, check, run_input, &BTreeMap::new())
}

/// Export with Gate F SDK emit map.
pub fn export_to_directory_with_sdk(
    project: &Project,
    out_dir: &Path,
    check: bool,
    run_input: Option<&[u8]>,
    sdk_emits: &BTreeMap<String, SdkEmit>,
) -> Result<ExportReport, CompileError> {
    export_to_directory_with_policy(
        project,
        out_dir,
        check,
        run_input,
        sdk_emits,
        &CompilePolicy::dev(),
    )
}

/// Export with full compile policy (may generate Cargo.lock).
pub fn export_to_directory_with_policy(
    project: &Project,
    out_dir: &Path,
    check: bool,
    run_input: Option<&[u8]>,
    sdk_emits: &BTreeMap<String, SdkEmit>,
    policy: &CompilePolicy,
) -> Result<ExportReport, CompileError> {
    let report = compile_with_policy(project, sdk_emits, policy)?;
    write_compiled_workspace(&report, out_dir, policy, check, run_input)
}

fn write_compiled_workspace(
    report: &CompileReport,
    out_dir: &Path,
    policy: &CompilePolicy,
    check: bool,
    run_input: Option<&[u8]>,
) -> Result<ExportReport, CompileError> {
    let ws = &report.workspace;
    if out_dir.exists() {
        if out_dir.join("weavatrix.lock").exists() {
            fs::remove_dir_all(out_dir).map_err(|e| CompileError::Io(e.to_string()))?;
        } else if out_dir
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(true)
        {
            return Err(CompileError::Io(format!(
                "refusing to overwrite non-empty directory without weavatrix.lock: {}",
                out_dir.display()
            )));
        }
    }
    fs::create_dir_all(out_dir).map_err(|e| CompileError::Io(e.to_string()))?;
    ws.write_to(out_dir)?;

    // Digests sidecar
    if policy.compute_digests && !report.digests.is_empty() {
        let digests_json = serde_json::to_string_pretty(&report.digests)
            .map_err(|e| CompileError::Io(e.to_string()))?;
        fs::write(out_dir.join("weavatrix.digests.json"), digests_json)
            .map_err(|e| CompileError::Io(e.to_string()))?;
    }

    // Resolution explanation sidecar
    if !report.resolutions.is_empty() {
        let res_json = serde_json::to_string_pretty(&report.resolutions)
            .map_err(|e| CompileError::Io(e.to_string()))?;
        fs::write(out_dir.join("weavatrix.resolution.json"), res_json)
            .map_err(|e| CompileError::Io(e.to_string()))?;
    }

    let mut export = ExportReport {
        package_name: ws.package_name.clone(),
        out_dir: out_dir.display().to_string(),
        files: ws.files.len(),
        check_ok: None,
        run_output: None,
        run_stdout: None,
        digests: report.digests.clone(),
        cargo_lock_generated: false,
        policy_id: policy.id.clone(),
        target: None,
        host: None,
        wasm_path: None,
    };

    if policy.generate_cargo_lock {
        let status = Command::new("cargo")
            .arg("generate-lockfile")
            .arg("--quiet")
            .current_dir(out_dir)
            .status()
            .map_err(|e| CompileError::Cargo(e.to_string()))?;
        if !status.success() {
            return Err(CompileError::Cargo(
                "cargo generate-lockfile failed for exported project".into(),
            ));
        }
        export.cargo_lock_generated = out_dir.join("Cargo.lock").exists();
    }

    if check || run_input.is_some() {
        let status = Command::new("cargo")
            .arg("check")
            .arg("--quiet")
            .current_dir(out_dir)
            .status()
            .map_err(|e| CompileError::Cargo(e.to_string()))?;
        export.check_ok = Some(status.success());
        if !status.success() {
            return Err(CompileError::Cargo(
                "cargo check failed for exported project".into(),
            ));
        }
    }

    if let Some(input) = run_input {
        let input_path = out_dir.join(".wvx-input.bin");
        fs::write(&input_path, input).map_err(|e| CompileError::Io(e.to_string()))?;
        let output = Command::new("cargo")
            .arg("run")
            .arg("--quiet")
            .env("WVX_PIPELINE_INPUT_FILE", &input_path)
            .env_remove("WVX_PIPELINE_INPUT")
            .current_dir(out_dir)
            .output()
            .map_err(|e| CompileError::Cargo(e.to_string()))?;
        let _ = fs::remove_file(&input_path);
        if !output.status.success() {
            return Err(CompileError::Cargo(format!(
                "cargo run failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        export.run_output = Some(output.stdout.clone());
        export.run_stdout = String::from_utf8(output.stdout).ok();
    }

    Ok(export)
}

/// Optional wasm32-wasip1 sidecar (ADR-0006). Native remains the default.
///
/// Rejects SIMD / rayon implementations. Writes `.cargo/config.toml` and a
/// wasm-safe vendor `Cargo.toml` (no simd-json / sonic-rs / blake3 rayon).
/// `--check` runs `cargo check --target wasm32-wasip1` when the rustup target
/// is installed; it does **not** add a Wasm host or WIT runtime.
pub fn export_wasm_to_directory(
    project: &Project,
    out_dir: &Path,
    check: bool,
    sdk_emits: &BTreeMap<String, SdkEmit>,
    policy: &CompilePolicy,
) -> Result<ExportReport, CompileError> {
    let mut report = compile_with_policy(project, sdk_emits, policy)?;
    for impl_id in report.resolved_implementations.values() {
        if let Some(why) = adapters::wasm_incompatible_reason(impl_id) {
            return Err(CompileError::PolicyRejected(impl_id.clone(), why.into()));
        }
    }
    for f in &mut report.workspace.files {
        if f.relative_path == "vendor/wvx-adapters/Cargo.toml" {
            f.contents = vendor::standalone_adapters_cargo_toml_wasm();
        }
    }
    report.workspace.files.push(GeneratedFile {
        relative_path: ".cargo/config.toml".into(),
        contents: format!("[build]\ntarget = \"{WASM_TARGET}\"\n"),
    });
    let sidecar = serde_json::json!({
        "target": WASM_TARGET,
        "host": false,
        "native_default": true,
        "rejected": [
            "simd-json.parse@1",
            "sonic-rs.parse@1",
            "blake3.blake3-parallel@1"
        ],
        "note": "Optional sidecar (ADR-0006). Host is `wvx run-wasm` via wasmtime CLI — not an embedded VM or WIT."
    });
    report.workspace.files.push(GeneratedFile {
        relative_path: "weavatrix.wasm.json".into(),
        contents: format!("{sidecar}\n"),
    });
    if policy.compute_digests {
        report.digests = digest_workspace(&report.workspace);
    }
    // Do not generate Cargo.lock against wasm32 unless the target is present;
    // lockfile generation would fail closed when rustup target is missing.
    let mut wasm_policy = policy.clone();
    wasm_policy.generate_cargo_lock = false;
    let mut export = write_compiled_workspace(&report, out_dir, &wasm_policy, false, None)?;
    export.target = Some(WASM_TARGET.into());
    if check {
        if !wasm_target_installed() {
            return Err(CompileError::Cargo(format!(
                "rustup target `{WASM_TARGET}` is not installed; rustup target add {WASM_TARGET}"
            )));
        }
        let status = Command::new("cargo")
            .args(["check", "--quiet", "--target", WASM_TARGET])
            .current_dir(out_dir)
            .status()
            .map_err(|e| CompileError::Cargo(e.to_string()))?;
        export.check_ok = Some(status.success());
        if !status.success() {
            return Err(CompileError::Cargo(format!(
                "cargo check --target {WASM_TARGET} failed for exported project"
            )));
        }
    }
    Ok(export)
}

/// Thin WASI host: build the sidecar and run it with the **wasmtime CLI**.
///
/// This is not an embedded wasmtime/cranelift VM and not a WIT component
/// runtime. Fail-closed when `wasm32-wasip1` or `wasmtime` is missing.
pub fn run_wasm_in_directory(
    project: &Project,
    out_dir: &Path,
    input: &[u8],
    sdk_emits: &BTreeMap<String, SdkEmit>,
    policy: &CompilePolicy,
) -> Result<ExportReport, CompileError> {
    require_wasm_host_tools()?;
    let mut export = export_wasm_to_directory(project, out_dir, false, sdk_emits, policy)?;
    let status = Command::new("cargo")
        .args(["build", "--quiet", "--target", WASM_TARGET])
        .current_dir(out_dir)
        .status()
        .map_err(|e| CompileError::Cargo(e.to_string()))?;
    if !status.success() {
        return Err(CompileError::Cargo(format!(
            "cargo build --target {WASM_TARGET} failed for wasm host"
        )));
    }
    let rel_wasm = format!("target/{WASM_TARGET}/debug/{}.wasm", export.package_name);
    let wasm = out_dir.join(&rel_wasm);
    if !wasm.is_file() {
        return Err(CompileError::Cargo(format!(
            "wasm artifact missing at {}",
            wasm.display()
        )));
    }
    let input_rel = ".wvx-input.bin";
    fs::write(out_dir.join(input_rel), input).map_err(|e| CompileError::Io(e.to_string()))?;
    let output = Command::new("wasmtime")
        .args([
            "--dir",
            ".",
            "--env",
            &format!("WVX_PIPELINE_INPUT_FILE={input_rel}"),
            &rel_wasm,
        ])
        .current_dir(out_dir)
        .output()
        .map_err(|e| CompileError::Cargo(format!("wasmtime CLI: {e}")))?;
    let _ = fs::remove_file(out_dir.join(input_rel));
    if !output.status.success() {
        return Err(CompileError::Cargo(format!(
            "wasmtime failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    export.check_ok = Some(true);
    export.host = Some("wasmtime-cli".into());
    export.wasm_path = Some(rel_wasm);
    export.run_output = Some(output.stdout.clone());
    export.run_stdout = String::from_utf8(output.stdout).ok();
    Ok(export)
}

pub fn require_wasm_host_tools() -> Result<(), CompileError> {
    if !wasm_target_installed() {
        return Err(CompileError::Cargo(format!(
            "rustup target `{WASM_TARGET}` is not installed; rustup target add {WASM_TARGET}"
        )));
    }
    if !wasmtime_cli_available() {
        return Err(CompileError::Cargo(
            "wasmtime CLI is not on PATH (Loom hosts via wasmtime; not an embedded VM or WIT). Install: https://wasmtime.dev/"
                .into(),
        ));
    }
    Ok(())
}

pub fn wasmtime_cli_available() -> bool {
    Command::new("wasmtime")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn wasm_target_installed() -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(WASM_TARGET))
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportReport {
    pub package_name: String,
    pub out_dir: String,
    pub files: usize,
    pub check_ok: Option<bool>,
    /// Raw pipeline stdout (binary-safe). Prefer this over `run_stdout`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_output: Option<Vec<u8>>,
    /// UTF-8 interpretation of stdout when valid; absent for binary payloads.
    pub run_stdout: Option<String>,
    #[serde(default)]
    pub digests: BTreeMap<String, String>,
    #[serde(default)]
    pub cargo_lock_generated: bool,
    #[serde(default)]
    pub policy_id: String,
    /// Set on wasm sidecar export (`wasm32-wasip1`). Absent for native.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// `wasmtime-cli` when `run-wasm` hosted the artifact. Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm_path: Option<String>,
}

/// instance_id → implementation_id + resolution explanations
fn resolve_implementations_explained(
    project: &Project,
    sdk_emits: &BTreeMap<String, SdkEmit>,
    policy: &CompilePolicy,
) -> Result<(BTreeMap<String, String>, Vec<ResolveDecision>), CompileError> {
    let mut map = BTreeMap::new();
    let mut decisions = Vec::new();
    let resolver = policy.to_resolver_policy();
    let profile = &policy.target_profile;

    for instance in &project.instances {
        let cap = instance.capability.as_key();
        let explicit = instance.implementation.clone().filter(|s| !s.is_empty());

        // Explainable resolve when we have a registry pool
        if !policy.implementations.is_empty() {
            let mut decision =
                resolve_implementation(&cap, &policy.implementations, profile, &resolver);
            if let Some(ref want) = explicit {
                // Explicit selection: still filter by policy
                if let Some(imp) = policy.implementations.iter().find(|i| i.full_id() == *want) {
                    if !policy.allow_candidate
                        && matches!(
                            imp.status,
                            LifecycleStatus::Candidate | LifecycleStatus::InventoryOnly
                        )
                    {
                        return Err(CompileError::PolicyRejected(
                            want.clone(),
                            format!("release policy forbids lifecycle `{}`", imp.status.as_str()),
                        ));
                    }
                    if policy.require_conformance_pass
                        && imp.evidence.conformance == wvx_ir::AxisFact::Fail
                    {
                        return Err(CompileError::PolicyRejected(
                            want.clone(),
                            "conformance evidence is fail".into(),
                        ));
                    }
                    decision.chosen = Some(want.clone());
                    decision
                        .explanation
                        .push(format!("explicit instance selection `{want}`"));
                } else {
                    decision.explanation.push(format!(
                        "explicit `{want}` not in policy implementations pool"
                    ));
                    decision.chosen = Some(want.clone());
                }
            } else if decision.chosen.is_none() {
                // Fall back to default catalog if pool empty for this cap
                if let Some(def) = default_implementation(&cap) {
                    decision.chosen = Some(def.into());
                    decision
                        .explanation
                        .push(format!("fallback default catalog `{def}`"));
                }
            }
            let impl_id = decision.chosen.clone().ok_or_else(|| {
                CompileError::UnsupportedImplementation("(none)".into(), cap.clone())
            })?;
            ensure_supported(&impl_id, &cap, sdk_emits)?;
            map.insert(instance.id.clone(), impl_id);
            decisions.push(decision);
            continue;
        }

        // No pool: explicit or default
        let impl_id = explicit
            .or_else(|| default_implementation(&cap).map(str::to_string))
            .ok_or_else(|| CompileError::UnsupportedImplementation("(none)".into(), cap.clone()))?;
        ensure_supported(&impl_id, &cap, sdk_emits)?;

        decisions.push(ResolveDecision {
            capability_key: cap.clone(),
            policy_id: policy.id.clone(),
            profile_id: profile.id.clone(),
            chosen: Some(impl_id.clone()),
            ranked: vec![impl_id.clone()],
            explanation: vec![
                format!("resolve capability `{cap}` (no registry pool)"),
                format!("chosen `{impl_id}`"),
            ],
            rejected: Vec::new(),
        });
        map.insert(instance.id.clone(), impl_id);
    }
    Ok((map, decisions))
}

fn ensure_supported(
    impl_id: &str,
    cap: &str,
    sdk_emits: &BTreeMap<String, SdkEmit>,
) -> Result<(), CompileError> {
    let sdk = sdk_emits.get(impl_id);
    if !adapters::supports(impl_id, cap, sdk) {
        if !adapters::is_passthrough_io(cap) {
            return Err(CompileError::UnsupportedImplementation(
                impl_id.into(),
                cap.into(),
            ));
        }
    }
    Ok(())
}

pub fn sanitize_pkg_name(id: &str) -> String {
    let mut out = String::new();
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() || out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert_str(0, "wvx_");
    }
    out
}

/// Re-export for hosts that only need structural validate before compile.
pub fn structural_validate(project: &Project) -> ValidationReport {
    validate_project(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::{
        AxisFact, CapabilityRef, Implementation, ImplementationEvidence, PROJECT_SCHEMA_VERSION,
    };

    #[test]
    fn compiles_pilot_fixture() {
        let text = include_str!("../../../fixtures/pilot-json-pipeline.wvx.json");
        let project: Project = serde_json::from_str(text).unwrap();
        assert_eq!(project.schema_version, PROJECT_SCHEMA_VERSION);
        let report =
            compile_with_policy(&project, &BTreeMap::new(), &CompilePolicy::dev()).unwrap();
        let ws = &report.workspace;
        assert!(ws.files.iter().any(|f| f.relative_path == "src/main.rs"));
        assert!(ws
            .files
            .iter()
            .any(|f| f.relative_path == "src/generated_pipeline.rs"));
        let pipeline = ws
            .files
            .iter()
            .find(|f| f.relative_path == "src/generated_pipeline.rs")
            .unwrap();
        assert!(pipeline.contents.contains("run_pipeline"));
        assert!(pipeline.contents.contains("run_pipeline_read"));
        assert!(pipeline.contents.contains("read_to_end"));
        assert!(pipeline
            .contents
            .contains("wvx_adapters::serde_json_parse_owned"));
        assert!(!pipeline.contents.contains("not yet linked"));
        assert!(ws
            .files
            .iter()
            .any(|f| f.relative_path.starts_with("vendor/wvx-adapters/")));
        assert!(!report.digests.is_empty());
        assert!(!report.resolutions.is_empty());
        assert!(report.digests.contains_key("weavatrix.lock"));
    }

    #[test]
    fn compile_release_rejects_empty_verified_pool() {
        let text = include_str!("../../../fixtures/pilot-json-pipeline.wvx.json");
        let project: Project = serde_json::from_str(text).unwrap();
        let err = compile_release(&project, &[], &BTreeMap::new()).unwrap_err();
        assert!(err.to_string().contains("VerifiedImplementation"), "{err}");
    }

    #[test]
    fn release_policy_rejects_candidate() {
        let text = include_str!("../../../fixtures/pilot-json-pipeline.wvx.json");
        let mut project: Project = serde_json::from_str(text).unwrap();
        // Force candidate on parse
        if let Some(inst) = project.instances.iter_mut().find(|i| i.id == "parse") {
            inst.implementation = Some("serde-json.parse-owned@1".into());
        }
        let mut policy = CompilePolicy::release();
        policy.implementations = vec![Implementation {
            id: "serde-json.parse-owned".into(),
            version: "1".into(),
            capability: CapabilityRef::new("data.json.parse", "1"),
            source: Default::default(),
            adapter: None,
            status: LifecycleStatus::Candidate,
            evidence: ImplementationEvidence {
                conformance: AxisFact::Pass,
                build: AxisFact::Pass,
                ..Default::default()
            },
            notes: None,
            sdk: None,
            conformance_profile: None,
            evidence_artifact: None,
            source_ref: None,
        }];
        // Also need other impls or leave them without pool entries (explicit still works)
        // path_set and serialize without pool entries use explicit from project
        let err = compile_with_policy(&project, &BTreeMap::new(), &policy).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("candidate") || msg.contains("policy") || msg.contains("release"),
            "{msg}"
        );
    }

    #[test]
    fn export_pilot_cargo_check_and_run() {
        let text = include_str!("../../../fixtures/pilot-json-pipeline.wvx.json");
        let project: Project = serde_json::from_str(text).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "wvx-export-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let report =
            export_to_directory(&project, &dir, true, Some(br#"{"hello":"world"}"#)).unwrap();
        assert_eq!(report.check_ok, Some(true));
        let stdout = report.run_stdout.unwrap();
        let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
        assert_eq!(v["hello"], "world");
        assert_eq!(v["tag"], "loom");
        assert!(dir.join("weavatrix.digests.json").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_wasm_writes_sidecar_and_rejects_simd() {
        let text = include_str!("../../../fixtures/pilot-json-pipeline.wvx.json");
        let mut project: Project = serde_json::from_str(text).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "wvx-wasm-export-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let report = export_wasm_to_directory(
            &project,
            &dir,
            false,
            &BTreeMap::new(),
            &CompilePolicy::dev(),
        )
        .unwrap();
        assert_eq!(report.target.as_deref(), Some(WASM_TARGET));
        assert!(dir.join(".cargo/config.toml").exists());
        assert!(dir.join("weavatrix.wasm.json").exists());
        let cargo = fs::read_to_string(dir.join("vendor/wvx-adapters/Cargo.toml")).unwrap();
        assert!(
            !cargo.contains("simd-json"),
            "wasm vendor must not depend on simd-json"
        );
        assert!(!cargo.contains("sonic-rs"));
        let cfg = fs::read_to_string(dir.join(".cargo/config.toml")).unwrap();
        assert!(cfg.contains(WASM_TARGET));
        let _ = fs::remove_dir_all(&dir);

        if let Some(inst) = project.instances.iter_mut().find(|i| i.id == "parse") {
            inst.implementation = Some("simd-json.parse@1".into());
        }
        let err = export_wasm_to_directory(
            &project,
            &std::env::temp_dir().join(format!(
                "wvx-wasm-reject-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )),
            false,
            &BTreeMap::new(),
            &CompilePolicy::dev(),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("simd-json") || err.to_string().contains("wasm"),
            "{err}"
        );
    }

    #[test]
    fn hash_export_streams_read() {
        let text = include_str!("../../../fixtures/pilot-hash-pipeline.wvx.json");
        let project: Project = serde_json::from_str(text).unwrap();
        let report =
            compile_with_policy(&project, &BTreeMap::new(), &CompilePolicy::dev()).unwrap();
        let pipeline = report
            .workspace
            .files
            .iter()
            .find(|f| f.relative_path == "src/generated_pipeline.rs")
            .unwrap();
        assert!(
            pipeline
                .contents
                .contains("sha2_sha256::digest_read(reader)"),
            "{}",
            pipeline.contents
        );
        assert!(!pipeline.contents.contains("reader.read_to_end(&mut input)"));
    }

    #[test]
    fn hash_pair_emits_tuple_and_named_sinks() {
        let text = include_str!("../../../fixtures/pilot-hash-pair-pipeline.wvx.json");
        let project: Project = serde_json::from_str(text).unwrap();
        let report =
            compile_with_policy(&project, &BTreeMap::new(), &CompilePolicy::dev()).unwrap();
        let pipeline = report
            .workspace
            .files
            .iter()
            .find(|f| f.relative_path == "src/generated_pipeline.rs")
            .unwrap();
        assert!(
            pipeline.contents.contains("digest_hex"),
            "{}",
            pipeline.contents
        );
        assert!(pipeline.contents.contains("run_pipeline_named"));
        assert!(pipeline.contents.contains("out_digest"));
        assert!(pipeline.contents.contains("out_hex"));
        assert!(
            pipeline.contents.contains("let (") && pipeline.contents.contains(") = "),
            "expected tuple unpack: {}",
            pipeline.contents
        );
    }

    #[test]
    fn run_wasm_fail_closed_without_host_tools() {
        if wasm_target_installed() && wasmtime_cli_available() {
            return;
        }
        let text = include_str!("../../../fixtures/pilot-json-pipeline.wvx.json");
        let project: Project = serde_json::from_str(text).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "wvx-wasm-host-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let err = run_wasm_in_directory(
            &project,
            &dir,
            br#"{"hello":"world"}"#,
            &BTreeMap::new(),
            &CompilePolicy::dev(),
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("wasm32-wasip1") || msg.contains("wasmtime"),
            "{msg}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
