//! Single semantic API used by product hosts (CLI, HTTP/`loom-server`) and the
//! optional **agent-only** MCP adapter (`wvx-mcp`).
//!
//! Studio never talks MCP — it uses HTTP into `loom-server`. Hosts must not
//! re-implement validation or graph rules.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use std::path::{Path, PathBuf};
use std::process::Command;
use wvx_compiler_rust::{
    compile_to_rust_with_sdk, export_to_directory_with_sdk, ExportReport, GeneratedWorkspace,
};
use wvx_ir::SdkEmit;
use wvx_forge::{
    compile_adapters_batch, default_workspace_root, draft_adapters_with_ontology,
    draft_from_extract_with_ontology, extract_from_facts, extract_public_api, facts_from_extract,
    inventory_path, load_facts_file, match_candidates, parse_facts_json, run_gate_c_pilot,
    write_draft_files, write_facts_file, CompileBatchReport, DraftReport, ExtractReport,
    ForgeError, GateCReport, InventoryReport, MatchReport, OntologyCapability, OntologyPort,
    WeavatrixFactsBundle,
};
use wvx_ir::Project;
use wvx_project_graph::{
    apply_graph_patch, propose_json_pipeline_patch_relative, GraphPatch, PatchApplyResult,
    PatchError,
};
use wvx_conformance::{run_pilot_bench, BenchReport};
use wvx_registry_client::{
    admit_implementation, CapabilityHit, ImplementationHit, InstallCandidateResult, LocalRegistry,
    RegistryError, RegistrySummary, AdmitRequest, AdmitResult, AdmissionReport,
};
use std::collections::BTreeMap;
use wvx_runtime::{
    apply_implementation_overrides, list_pilot_implementations, run_project, HandlerRegistry,
    RunResult, RuntimeError, WvxValueMap,
};
use wvx_types::WvxValue;
use wvx_validator::{validate_project, ValidationReport};
use wvx_cortex::{propose_from_intent, CortexError, IntentProposeResult};

pub const PROTOCOL_VERSION: &str = "0.1";

#[derive(Debug, Error)]
pub enum BusError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error("invalid project: {0}")]
    InvalidProject(String),
    #[error("compile failed: {0}")]
    Compile(String),
    #[error("run failed: {0}")]
    Run(String),
    #[error("io: {0}")]
    Io(String),
    #[error("forge: {0}")]
    Forge(String),
    #[error("patch: {0}")]
    Patch(String),
    #[error("cortex: {0}")]
    Cortex(String),
}

impl From<CortexError> for BusError {
    fn from(value: CortexError) -> Self {
        BusError::Cortex(value.to_string())
    }
}

impl From<PatchError> for BusError {
    fn from(value: PatchError) -> Self {
        BusError::Patch(value.to_string())
    }
}

impl From<ForgeError> for BusError {
    fn from(value: ForgeError) -> Self {
        BusError::Forge(value.to_string())
    }
}

impl From<RuntimeError> for BusError {
    fn from(value: RuntimeError) -> Self {
        BusError::Run(value.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusResponse<T> {
    pub protocol_version: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
}

impl<T> BusResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.into(),
            ok: true,
            data: Some(data),
            diagnostics: Vec::new(),
        }
    }

    pub fn err(messages: Vec<String>) -> BusResponse<T> {
        BusResponse {
            protocol_version: PROTOCOL_VERSION.into(),
            ok: false,
            data: None,
            diagnostics: messages,
        }
    }
}

/// Fill missing capability contracts from a local registry (no-op when `None`).
pub fn hydrate_project(project: &mut Project, registry: Option<&LocalRegistry>) -> Result<usize, BusError> {
    match registry {
        Some(reg) => Ok(reg.hydrate_project_capabilities(project)?),
        None => Ok(0),
    }
}

/// Validate a project document.
pub fn project_validate(project: &Project) -> BusResponse<ValidationReport> {
    project_validate_hydrated(project, None)
}

/// Validate after optionally hydrating capability contracts from a registry.
pub fn project_validate_hydrated(
    project: &Project,
    registry: Option<&LocalRegistry>,
) -> BusResponse<ValidationReport> {
    let mut project = project.clone();
    if let Err(e) = hydrate_project(&mut project, registry) {
        return BusResponse::err(vec![format!("registry hydrate failed: {e}")]);
    }
    let report = validate_project(&project);
    if report.is_ok() {
        BusResponse::ok(report)
    } else {
        let messages = report.errors().map(|d| d.message.clone()).collect();
        let mut resp = BusResponse::err(messages);
        resp.data = Some(report);
        resp
    }
}

/// Compile a project to a generated Rust package (in memory).
pub fn project_export_rust(project: &Project) -> Result<BusResponse<GeneratedWorkspace>, BusError> {
    project_export_rust_hydrated(project, None)
}

/// Compile after optionally hydrating capability contracts from a registry.
pub fn project_export_rust_hydrated(
    project: &Project,
    registry: Option<&LocalRegistry>,
) -> Result<BusResponse<GeneratedWorkspace>, BusError> {
    let mut project = project.clone();
    hydrate_project(&mut project, registry)?;
    let sdk = sdk_emits_from_registry(registry);
    match compile_to_rust_with_sdk(&project, &sdk) {
        Ok(ws) => Ok(BusResponse::ok(ws)),
        Err(e) => Err(BusError::Compile(e.to_string())),
    }
}

/// Export a project to a directory; optionally `cargo check` and run.
pub fn project_export_to_dir(
    project: &Project,
    out_dir: &Path,
    check: bool,
    run_input: Option<&[u8]>,
) -> Result<BusResponse<ExportReport>, BusError> {
    project_export_to_dir_with_registry(project, out_dir, check, run_input, None)
}

/// Export with registry SDK emit map (Gate F).
pub fn project_export_to_dir_with_registry(
    project: &Project,
    out_dir: &Path,
    check: bool,
    run_input: Option<&[u8]>,
    registry: Option<&LocalRegistry>,
) -> Result<BusResponse<ExportReport>, BusError> {
    let mut project = project.clone();
    let _ = hydrate_project(&mut project, registry);
    let sdk = sdk_emits_from_registry(registry);
    match export_to_directory_with_sdk(&project, out_dir, check, run_input, &sdk) {
        Ok(report) => Ok(BusResponse::ok(report)),
        Err(e) => Err(BusError::Compile(e.to_string())),
    }
}

/// Run a project in the playground with pilot handlers.
///
/// `input_bytes` seeds the entrypoint `bytes` port (typical for the JSON pilot).
/// `impl_overrides` maps instance id → implementation id (capability graph unchanged).
pub fn project_run(
    project: &Project,
    input_bytes: Vec<u8>,
    impl_overrides: &BTreeMap<String, String>,
) -> Result<BusResponse<RunResult>, BusError> {
    project_run_hydrated(project, input_bytes, impl_overrides, None)
}

/// Run after optionally hydrating missing capability contracts from a registry.
///
/// Studio often stores instances without embedding full capability contracts; the
/// HTTP host should pass the open registry so validation/run still succeed.
pub fn project_run_hydrated(
    project: &Project,
    input_bytes: Vec<u8>,
    impl_overrides: &BTreeMap<String, String>,
    registry: Option<&LocalRegistry>,
) -> Result<BusResponse<RunResult>, BusError> {
    let mut project = project.clone();
    hydrate_project(&mut project, registry)?;
    apply_implementation_overrides(&mut project, impl_overrides);
    let handlers = playground_handlers();
    let mut seed = WvxValueMap::new();
    seed.insert("bytes".into(), WvxValue::Bytes(input_bytes));
    let result = run_project(&project, &handlers, seed)?;
    Ok(BusResponse::ok(result))
}

/// I/O (with_pilot) + all SDK plugins: pilot adapters + external Gate F demo.
fn playground_handlers() -> HandlerRegistry {
    wvx_adapters::register_pilot_plugins();
    wvx_adapter_external_demo::register();
    wvx_component_sdk::registry_with_pilot_and_plugins()
}

fn sdk_emits_from_registry(registry: Option<&LocalRegistry>) -> BTreeMap<String, SdkEmit> {
    let mut map = BTreeMap::new();
    // Built-in pilot templates always available for export without registry.
    for id in wvx_compiler_rust::known_implementation_ids() {
        if let Some(sdk) = wvx_compiler_rust::adapters_built_in_sdk_emit(id) {
            map.insert(id.to_string(), sdk);
        }
    }
    if let Some(reg) = registry {
        if let Ok(impls) = reg.list_implementations() {
            for imp in impls {
                if let Some(sdk) = imp.sdk.as_ref().and_then(|s| s.emit.clone()) {
                    map.insert(imp.full_id(), sdk);
                }
            }
        }
    }
    map
}

/// List pilot playground implementations (capability + implementation id).
pub fn implementations_list() -> BusResponse<Vec<ImplementationInfo>> {
    let items = list_pilot_implementations()
        .into_iter()
        .map(|p| ImplementationInfo {
            implementation_id: p.implementation_id.into(),
            capability: p.capability_key.into(),
            label: p.label.into(),
        })
        .collect();
    BusResponse::ok(items)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationInfo {
    pub implementation_id: String,
    pub capability: String,
    pub label: String,
}

/// Registry root summary.
pub fn registry_summary(registry: &LocalRegistry) -> Result<BusResponse<RegistrySummary>, BusError> {
    Ok(BusResponse::ok(registry.summary()?))
}

/// Search capabilities in a local registry (substring, case-insensitive).
pub fn registry_search(
    registry: &LocalRegistry,
    query: &str,
) -> Result<BusResponse<Vec<CapabilityHit>>, BusError> {
    Ok(BusResponse::ok(registry.search_capabilities(query)?))
}

/// Search or list implementations; optional capability filter `data.json.parse@1`.
pub fn registry_implementations(
    registry: &LocalRegistry,
    capability: Option<&str>,
    query: &str,
) -> Result<BusResponse<Vec<ImplementationHit>>, BusError> {
    let hits = if let Some(cap) = capability {
        registry
            .implementations_for_capability(cap)?
            .into_iter()
            .map(wvx_registry_client::hit_from_implementation)
            .filter(|h| {
                let q = query.trim().to_ascii_lowercase();
                q.is_empty()
                    || h.full_id.to_ascii_lowercase().contains(&q)
                    || h.package.to_ascii_lowercase().contains(&q)
                    || h.status.to_ascii_lowercase().contains(&q)
            })
            .collect()
    } else {
        registry.search_implementations(query)?
    };
    Ok(BusResponse::ok(hits))
}

/// Audit declared lifecycle status vs multi-fact evidence (overclaim = fail).
///
/// Automated consistency check (ADR-0007/0008). Human admit is separate.
pub fn registry_admission_audit(
    registry: &LocalRegistry,
) -> Result<BusResponse<AdmissionReport>, BusError> {
    Ok(BusResponse::ok(registry.audit_admission()?))
}

/// Pilot microbench for Gate E (`benchmark` evidence axis).
pub fn pilot_bench(iterations: u32, warmup: u32) -> BusResponse<BenchReport> {
    BusResponse::ok(run_pilot_bench(iterations, warmup))
}

/// Human Gate E admit (fail-closed). See `docs/go-no-go-e-pilot.md`.
pub fn registry_human_admit(
    registry: &LocalRegistry,
    req: AdmitRequest,
) -> Result<BusResponse<AdmitResult>, BusError> {
    let Some(imp) = registry.find_implementation(&req.implementation_id)? else {
        return Ok(BusResponse::err(vec![format!(
            "implementation not found: {}",
            req.implementation_id
        )]));
    };
    let result = admit_implementation(registry.root(), imp, &req)?;
    if result.ok {
        Ok(BusResponse::ok(result))
    } else {
        let mut resp = BusResponse::ok(result.clone());
        resp.ok = false;
        resp.diagnostics = result.findings.clone();
        Ok(resp)
    }
}

/// Inspect one capability or implementation by key (`data.json.parse@1` or impl full id).
pub fn registry_inspect(
    registry: &LocalRegistry,
    key: &str,
) -> Result<BusResponse<serde_json::Value>, BusError> {
    if let Some(cap) = registry.find_capability_key(key)? {
        return Ok(BusResponse::ok(serde_json::to_value(cap).map_err(|e| {
            BusError::Io(e.to_string())
        })?));
    }
    if let Some(imp) = registry.find_implementation(key)? {
        return Ok(BusResponse::ok(serde_json::to_value(imp).map_err(|e| {
            BusError::Io(e.to_string())
        })?));
    }
    Ok(BusResponse::err(vec![format!("not found: {key}")]))
}

/// Load a project JSON file from disk.
pub fn load_project_path(path: &std::path::Path) -> Result<Project, BusError> {
    let text = std::fs::read_to_string(path).map_err(|e| BusError::Io(e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| BusError::Io(e.to_string()))
}

/// Static package inventory (Forge stage 1 — no code execution).
pub fn forge_inventory(path: &Path) -> Result<BusResponse<InventoryReport>, BusError> {
    let report = inventory_path(path)?;
    Ok(BusResponse::ok(report))
}

/// Public API extract + candidate shapes (Forge stage 2 — static only).
///
/// **Bootstrap** path: local Cargo AST. Prefer [`forge_facts_to_extract`] when
/// Weavatrix facts are available (ADR-0012).
pub fn forge_extract(path: &Path) -> Result<BusResponse<ExtractReport>, BusError> {
    let report = extract_public_api(path)?;
    Ok(BusResponse::ok(report))
}

/// Convert a Weavatrix facts bundle → extract-shaped candidates (no filesystem walk).
pub fn forge_facts_to_extract(
    facts: &WeavatrixFactsBundle,
) -> Result<BusResponse<ExtractReport>, BusError> {
    let report = extract_from_facts(facts);
    Ok(BusResponse::ok(report))
}

/// Load Weavatrix facts JSON from disk → extract report.
pub fn forge_facts_file(path: &Path) -> Result<BusResponse<ExtractReport>, BusError> {
    let bundle = load_facts_file(path).map_err(|e| BusError::Forge(e.to_string()))?;
    forge_facts_to_extract(&bundle)
}

/// Parse Weavatrix facts JSON text → extract report.
pub fn forge_facts_json(text: &str) -> Result<BusResponse<ExtractReport>, BusError> {
    let bundle = parse_facts_json(text).map_err(|e| BusError::Forge(e))?;
    forge_facts_to_extract(&bundle)
}

/// Export bootstrap AST extract as a Weavatrix-compatible facts JSON file.
pub fn forge_export_facts(
    crate_path: &Path,
    out_path: &Path,
) -> Result<BusResponse<WeavatrixFactsBundle>, BusError> {
    let extract = extract_public_api(crate_path)?;
    let bundle = facts_from_extract(&extract, "bootstrap-export");
    write_facts_file(&bundle, out_path).map_err(|e| BusError::Forge(e.to_string()))?;
    Ok(BusResponse::ok(bundle))
}

/// Adapter drafts from extract (Forge stage 3 — static only, `inventory_only`).
///
/// Optional `name_filter` substring on function names. Optional `out_dir` writes
/// capability.json / implementation.json / adapter_stub.rs per draft.
/// When `registry` is set, FORGE-007 maps candidates onto existing capabilities.
pub fn forge_draft(
    path: &Path,
    name_filter: Option<&str>,
    out_dir: Option<&Path>,
    registry: Option<&LocalRegistry>,
) -> Result<BusResponse<DraftReport>, BusError> {
    let ontology = ontology_from_registry(registry)?;
    let mut report = draft_adapters_with_ontology(path, name_filter, &ontology)?;
    if let Some(dir) = out_dir {
        match write_draft_files(&report, dir) {
            Ok(n) => report
                .notes
                .push(format!("Wrote {n} draft package(s) under {}", dir.display())),
            Err(e) => return Err(BusError::Forge(e.to_string())),
        }
    }
    Ok(BusResponse::ok(report))
}

/// Draft + install selected Forge drafts into the local registry as **candidates**.
///
/// Never admits. By default only drafts that **reuse** an existing capability
/// (`exact_shape` / `compatible_shape`) are installed, so Library grows with
/// new implementations under known capabilities — not raw crates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterCandidatesReport {
    pub package_name: String,
    pub considered: usize,
    pub installed: Vec<InstallCandidateResult>,
    pub skipped: Vec<String>,
    pub notes: Vec<String>,
}

/// One hit from `cargo search` (crates.io), optionally with a local registry cache path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoSearchHit {
    pub name: String,
    pub version: String,
    pub description: String,
    /// Best-effort path under `$CARGO_HOME/registry/src` when the crate is already cached.
    pub local_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoSearchReport {
    pub query: String,
    pub hits: Vec<CargoSearchHit>,
    pub notes: Vec<String>,
}

/// Interactive crates.io search via the host `cargo search` (not a product MCP).
///
/// Results may include a **local cache path** so Forge inventory can run without
/// downloading. Selecting a hit never puts a crate into the Capability Library
/// directly — inventory / match / register still apply.
pub fn forge_cargo_search(
    query: &str,
    limit: usize,
) -> Result<BusResponse<CargoSearchReport>, BusError> {
    let q = query.trim();
    if q.is_empty() {
        return Err(BusError::Forge("cargo search query is empty".into()));
    }
    let limit = limit.clamp(1, 20);
    let output = Command::new("cargo")
        .args(["search", q, "--limit", &limit.to_string()])
        .output()
        .map_err(|e| BusError::Forge(format!("failed to run cargo search: {e}")))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(BusError::Forge(format!(
            "cargo search failed: {}",
            err.trim()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut hits = Vec::new();
    let mut notes = Vec::new();
    for line in stdout.lines() {
        // serde = "1.0.210"    # description...
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((left, rest)) = line.split_once('=') else {
            continue;
        };
        let name = left.trim().to_string();
        let rest = rest.trim();
        let version = rest
            .trim_start_matches('"')
            .split('"')
            .next()
            .unwrap_or("")
            .to_string();
        let description = rest
            .split('#')
            .nth(1)
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        if name.is_empty() || version.is_empty() {
            continue;
        }
        let local_path = find_cached_crate_path(&name, &version);
        hits.push(CargoSearchHit {
            name,
            version,
            description,
            local_path,
        });
    }
    if hits.is_empty() {
        notes.push("No crates.io hits (or cargo search unavailable).".into());
    } else {
        let cached = hits.iter().filter(|h| h.local_path.is_some()).count();
        notes.push(format!(
            "{} hit(s); {cached} already in local cargo registry cache.",
            hits.len()
        ));
        notes.push(
            "Pick a hit with a local path to Inventory, or clone/path the crate first."
                .into(),
        );
    }
    Ok(BusResponse::ok(CargoSearchReport {
        query: q.to_string(),
        hits,
        notes,
    }))
}

fn find_cached_crate_path(name: &str, version: &str) -> Option<String> {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|h| PathBuf::from(h).join(".cargo"))
        })?;
    let src = cargo_home.join("registry").join("src");
    if !src.is_dir() {
        return None;
    }
    let prefix = format!("{name}-{version}");
    let prefix_name = format!("{name}-");
    let mut best: Option<PathBuf> = None;
    if let Ok(indexes) = std::fs::read_dir(&src) {
        for index in indexes.flatten() {
            let index_path = index.path();
            if !index_path.is_dir() {
                continue;
            }
            // Prefer exact version, else newest matching name- prefix.
            let exact = index_path.join(&prefix);
            if exact.is_dir() {
                return Some(exact.display().to_string());
            }
            if let Ok(crates) = std::fs::read_dir(&index_path) {
                for c in crates.flatten() {
                    let p = c.path();
                    let Some(fname) = p.file_name().and_then(|s| s.to_str()) else {
                        continue;
                    };
                    if fname.starts_with(&prefix_name) && p.is_dir() {
                        match &best {
                            None => best = Some(p),
                            Some(b) => {
                                if fname > b.file_name().and_then(|s| s.to_str()).unwrap_or("") {
                                    best = Some(p);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    best.map(|p| p.display().to_string())
}

/// When `only_matched` is true, only ontology-reusing mappings are written.
pub fn forge_register_candidates(
    path: &Path,
    name_filter: Option<&str>,
    registry: &LocalRegistry,
    only_matched: bool,
) -> Result<BusResponse<RegisterCandidatesReport>, BusError> {
    let ontology = ontology_from_registry(Some(registry))?;
    let draft = draft_adapters_with_ontology(path, name_filter, &ontology)?;
    let mut installed = Vec::new();
    let mut skipped = Vec::new();
    let mut notes = draft.notes.clone();

    for d in &draft.drafts {
        let reuses = d.mapping_kind == "exact_shape" || d.mapping_kind == "compatible_shape";
        if only_matched && !reuses {
            skipped.push(format!(
                "{} — skip (mapping={}, only_matched)",
                d.implementation_id, d.mapping_kind
            ));
            continue;
        }
        let write_cap = !reuses;
        match registry.install_forge_draft_candidate(
            &d.capability_json,
            &d.implementation_json,
            write_cap,
        ) {
            Ok(r) => installed.push(r),
            Err(e) => skipped.push(format!("{} — {e}", d.implementation_id)),
        }
    }

    notes.push(format!(
        "Registered {} candidate impl(s); skipped {}.",
        installed.len(),
        skipped.len()
    ));
    notes.push("Candidates are not admitted — run conformance before promote.".into());

    Ok(BusResponse::ok(RegisterCandidatesReport {
        package_name: draft.package_name,
        considered: draft.drafts.len(),
        installed,
        skipped,
        notes,
    }))
}

/// FORGE-007: match public `fn` candidates to registry capability ontology (static).
///
/// Bootstrap: extract from `path`. Prefer [`forge_match_facts`] when Weavatrix
/// facts are available.
pub fn forge_match(
    path: &Path,
    registry: Option<&LocalRegistry>,
) -> Result<BusResponse<MatchReport>, BusError> {
    let extract = extract_public_api(path)?;
    let ontology = ontology_from_registry(registry)?;
    let report = match_candidates(&extract.package_name, &extract.candidates, &ontology);
    Ok(BusResponse::ok(report))
}

/// FORGE-007 from **Weavatrix facts** (no bootstrap AST walk).
pub fn forge_match_facts(
    facts: &WeavatrixFactsBundle,
    registry: Option<&LocalRegistry>,
) -> Result<BusResponse<MatchReport>, BusError> {
    let extract = extract_from_facts(facts);
    let ontology = ontology_from_registry(registry)?;
    let report = match_candidates(&extract.package_name, &extract.candidates, &ontology);
    Ok(BusResponse::ok(report))
}

/// Draft adapters from Weavatrix facts (ontology match when registry set).
pub fn forge_draft_facts(
    facts: &WeavatrixFactsBundle,
    name_filter: Option<&str>,
    out_dir: Option<&Path>,
    registry: Option<&LocalRegistry>,
) -> Result<BusResponse<DraftReport>, BusError> {
    let extract = extract_from_facts(facts);
    let ontology = ontology_from_registry(registry)?;
    let root = facts
        .package_root
        .as_deref()
        .map(Path::new)
        .unwrap_or_else(|| Path::new("."));
    let mut report = draft_from_extract_with_ontology(&extract, name_filter, root, &ontology)?;
    if let Some(dir) = out_dir {
        match write_draft_files(&report, dir) {
            Ok(n) => report
                .notes
                .push(format!("Wrote {n} draft package(s) under {}", dir.display())),
            Err(e) => return Err(BusError::Forge(e.to_string())),
        }
    }
    report
        .notes
        .push("Draft source: Weavatrix facts (not bootstrap AST).".into());
    Ok(BusResponse::ok(report))
}

/// FORGE-008: generate compileable adapter crates (optional `cargo check`).
pub fn forge_compile(
    path: &Path,
    name_filter: Option<&str>,
    out_dir: &Path,
    check: bool,
    registry: Option<&LocalRegistry>,
) -> Result<BusResponse<CompileBatchReport>, BusError> {
    let ontology = ontology_from_registry(registry)?;
    let draft = draft_adapters_with_ontology(path, name_filter, &ontology)?;
    let report = compile_adapters_batch(
        path,
        &draft.package_name,
        &draft.drafts,
        out_dir,
        check,
        true,
    )?;
    Ok(BusResponse::ok(report))
}

/// Gate C pilot economics harness (fixture metrics; not production admission).
pub fn forge_gate_c(
    workspace_root: Option<&Path>,
    registry: Option<&LocalRegistry>,
    run_compile: bool,
) -> Result<BusResponse<GateCReport>, BusError> {
    let root = workspace_root
        .map(Path::to_path_buf)
        .unwrap_or_else(default_workspace_root);
    let ontology = ontology_from_registry(registry)?;
    let ontology_ref = if ontology.is_empty() {
        None
    } else {
        Some(ontology.as_slice())
    };
    let report = run_gate_c_pilot(&root, ontology_ref, run_compile)?;
    Ok(BusResponse::ok(report))
}

fn ontology_from_registry(
    registry: Option<&LocalRegistry>,
) -> Result<Vec<OntologyCapability>, BusError> {
    let Some(reg) = registry else {
        return Ok(Vec::new());
    };
    let caps = reg.list_capabilities()?;
    Ok(caps
        .into_iter()
        .map(|c| OntologyCapability {
            id: c.id,
            version: c.version,
            kind: c.kind,
            inputs: c
                .inputs
                .into_iter()
                .map(|p| OntologyPort {
                    id: p.id,
                    ty: p.ty.to_string(),
                })
                .collect(),
            outputs: c
                .outputs
                .into_iter()
                .map(|p| OntologyPort {
                    id: p.id,
                    ty: p.ty.to_string(),
                })
                .collect(),
        })
        .collect())
}

/// Propose a GraphPatch (rule-based pilot: JSON pipeline).
///
/// When `base` is provided, the proposal is **relative** to that project
/// (only missing pilot nodes/edges). When `None`, behaves like empty base.
pub fn graph_propose_patch(
    registry: Option<&LocalRegistry>,
    base: Option<&Project>,
) -> Result<BusResponse<GraphPatch>, BusError> {
    let caps = if let Some(reg) = registry {
        reg.list_capabilities()?
    } else {
        Vec::new()
    };
    let empty = Project::new("empty", "Empty");
    let project = base.unwrap_or(&empty);
    Ok(BusResponse::ok(propose_json_pipeline_patch_relative(
        project, &caps,
    )))
}

/// Propose a GraphPatch from natural-language intent (ops only).
///
/// Uses offline heuristics when possible; otherwise xAI (`XAI_API_KEY`).
/// Never auto-applies — caller must `graph_apply_patch` after user approval.
pub fn graph_propose_intent(
    registry: Option<&LocalRegistry>,
    project: &Project,
    intent: &str,
) -> Result<BusResponse<IntentProposeResult>, BusError> {
    let caps = if let Some(reg) = registry {
        reg.list_capabilities()?
    } else {
        Vec::new()
    };
    let result = propose_from_intent(intent, project, &caps)?;
    let mut resp = BusResponse::ok(result);
    if !resp.data.as_ref().map(|d| d.dry_run_ok).unwrap_or(false) {
        if let Some(d) = &resp.data {
            resp.diagnostics = d.dry_run_errors.clone();
            // Keep ok=true so clients still receive the patch for review (ghost).
        }
    }
    Ok(resp)
}

/// Validate a patch against a project (does not persist).
pub fn graph_validate_patch(
    project: &Project,
    patch: &GraphPatch,
) -> Result<BusResponse<PatchApplyResult>, BusError> {
    let result = apply_graph_patch(project, patch)?;
    let mut resp = if result.validation.is_ok() {
        BusResponse::ok(result)
    } else {
        let messages = result
            .validation
            .errors()
            .map(|d| d.message.clone())
            .collect();
        let mut r = BusResponse::err(messages);
        r.data = Some(result);
        r
    };
    let _ = &mut resp;
    Ok(resp)
}

/// Apply a patch and return the new project (caller persists).
pub fn graph_apply_patch(
    project: &Project,
    patch: &GraphPatch,
) -> Result<BusResponse<PatchApplyResult>, BusError> {
    graph_validate_patch(project, patch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::PROJECT_SCHEMA_VERSION;

    #[test]
    fn validate_empty_named_project() {
        let mut p = Project::new("x", "X");
        p.schema_version = PROJECT_SCHEMA_VERSION.into();
        let resp = project_validate(&p);
        assert!(resp.ok);
    }
}
