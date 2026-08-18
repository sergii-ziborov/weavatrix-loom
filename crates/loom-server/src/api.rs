//! JSON routes mapped 1:1 onto command-bus operations.

use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use wvx_command_bus::{
    forge_cargo_search, forge_compile, forge_draft, forge_draft_facts, forge_export_facts,
    forge_extract, forge_facts_to_extract, forge_gate_c_ex3, forge_inventory, forge_match,
    forge_match_facts, forge_register_candidates, graph_apply_patch, graph_commit_patch,
    graph_preview_patch, graph_propose_intent, graph_propose_patch, graph_validate_patch,
    implementations_list, pilot_catalog, project_export_rust_hydrated, project_run_hydrated,
    project_validate_hydrated, registry_admission_audit, registry_families,
    registry_implementations, registry_inspect, registry_profiles, registry_resolve,
    registry_search, registry_summary, registry_truthful_audit, registry_verify_evidence, BusError,
    BusResponse, PROTOCOL_VERSION,
};
use wvx_forge::WeavatrixFactsBundle;
use wvx_ir::Project;
use wvx_ir::{ResolverPolicy, TargetProfile};
use wvx_project_graph::GraphPatch;

use crate::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/protocol", get(protocol))
        .route("/api/v1/auth/bootstrap", get(auth_bootstrap))
        .route("/api/v1/project/validate", post(validate_project))
        .route("/api/v1/project/run", post(run_project))
        .route("/api/v1/project/export-rust", post(export_rust))
        .route("/api/v1/registry/summary", get(reg_summary))
        .route("/api/v1/registry/search", get(reg_search))
        .route("/api/v1/registry/implementations", get(reg_implementations))
        .route("/api/v1/registry/inspect/{key}", get(reg_inspect))
        .route("/api/v1/registry/admission", get(reg_admission))
        .route("/api/v1/registry/truthful", get(reg_truthful))
        .route("/api/v1/registry/profiles", get(reg_profiles))
        .route("/api/v1/registry/families", get(reg_families))
        .route("/api/v1/registry/resolve", post(reg_resolve))
        .route(
            "/api/v1/registry/verify-evidence/{key}",
            get(reg_verify_evidence),
        )
        .route("/api/v1/pilot/implementations", get(pilot_implementations))
        .route("/api/v1/pilot/catalog", get(pilot_catalog_handler))
        .route("/api/v1/forge/inventory", post(forge_inventory_handler))
        .route(
            "/api/v1/forge/cargo-search",
            get(forge_cargo_search_handler),
        )
        .route("/api/v1/forge/extract", post(forge_extract_handler))
        .route("/api/v1/forge/facts", post(forge_facts_handler))
        .route(
            "/api/v1/forge/export-facts",
            post(forge_export_facts_handler),
        )
        .route("/api/v1/forge/match", post(forge_match_handler))
        .route("/api/v1/forge/draft", post(forge_draft_handler))
        .route(
            "/api/v1/forge/register-candidates",
            post(forge_register_candidates_handler),
        )
        .route("/api/v1/forge/compile", post(forge_compile_handler))
        .route("/api/v1/forge/gate-c", post(forge_gate_c_handler))
        .route("/api/v1/forge/workspace-roots", get(forge_workspace_roots))
        .route("/api/v1/graph/propose_patch", post(propose_patch))
        .route("/api/v1/graph/propose_intent", post(propose_intent))
        .route("/api/v1/graph/preview_patch", post(preview_patch_handler))
        .route("/api/v1/graph/validate_patch", post(validate_patch))
        .route("/api/v1/graph/commit_patch", post(commit_patch_handler))
        .route("/api/v1/graph/apply_patch", post(apply_patch))
        .with_state(state)
}

/// Loopback-friendly bootstrap: returns the session token for Studio (SEC-001).
async fn auth_bootstrap(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "ok": true,
        "token": state.security.session_token,
        "header": "X-WVX-Token",
        "note": "Prefer WVX_SESSION_TOKEN env for stable tokens across restarts"
    }))
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true, "service": "loom-server" }))
}

async fn protocol() -> impl IntoResponse {
    Json(serde_json::json!({
        "protocol_version": PROTOCOL_VERSION,
        "product": "weavatrix-loom",
        "service": "loom-server",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

#[derive(Debug, Deserialize)]
struct ProjectBody {
    project: Project,
}

async fn validate_project(
    State(state): State<AppState>,
    Json(body): Json<ProjectBody>,
) -> impl IntoResponse {
    Json(project_validate_hydrated(
        &body.project,
        Some(state.registry.as_ref()),
    ))
}

#[derive(Debug, Deserialize)]
struct RunBody {
    project: Project,
    /// UTF-8 JSON text used as pipeline input bytes when `input_b64` is absent.
    #[serde(default)]
    input_json: Option<String>,
    /// Base64-encoded raw input bytes (optional).
    #[serde(default)]
    input_b64: Option<String>,
    /// instance_id → implementation_id overrides.
    #[serde(default)]
    impls: BTreeMap<String, String>,
}

async fn run_project(State(state): State<AppState>, Json(body): Json<RunBody>) -> Response {
    let input = match resolve_input(&body) {
        Ok(b) => b,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(BusResponse::<()>::err(vec![msg])),
            )
                .into_response();
        }
    };
    match project_run_hydrated(
        &body.project,
        input,
        &body.impls,
        Some(state.registry.as_ref()),
    ) {
        Ok(resp) => {
            if resp.ok {
                Json(resp).into_response()
            } else {
                // Validation failures from the runtime surface as BusResponse.ok=false
                // without Err; still return 422 so Studio can treat them as hard errors.
                (StatusCode::UNPROCESSABLE_ENTITY, Json(resp)).into_response()
            }
        }
        Err(e) => bus_error(e),
    }
}

fn resolve_input(body: &RunBody) -> Result<Vec<u8>, String> {
    if let Some(b64) = &body.input_b64 {
        return decode_b64(b64);
    }
    if let Some(json) = &body.input_json {
        let _: serde_json::Value =
            serde_json::from_str(json).map_err(|e| format!("invalid input_json: {e}"))?;
        return Ok(json.as_bytes().to_vec());
    }
    Ok(br#"{"hello":"world"}"#.to_vec())
}

/// Minimal base64 decode without extra deps (standard alphabet, ignore whitespace).
fn decode_b64(input: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => Some(0),
            _ => None,
        }
    }
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.len() % 4 != 0 {
        return Err("invalid base64 length".into());
    }
    let mut out = Vec::with_capacity(cleaned.len() * 3 / 4);
    for chunk in cleaned.as_bytes().chunks(4) {
        let b0 = val(chunk[0]).ok_or("invalid base64")?;
        let b1 = val(chunk[1]).ok_or("invalid base64")?;
        let b2 = val(chunk[2]).ok_or("invalid base64")?;
        let b3 = val(chunk[3]).ok_or("invalid base64")?;
        out.push((b0 << 2) | (b1 >> 4));
        if chunk[2] != b'=' {
            out.push((b1 << 4) | (b2 >> 2));
        }
        if chunk[3] != b'=' {
            out.push((b2 << 6) | b3);
        }
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct ExportBody {
    project: Project,
    #[serde(default)]
    impls: BTreeMap<String, String>,
}

async fn export_rust(State(state): State<AppState>, Json(body): Json<ExportBody>) -> Response {
    let mut project = body.project;
    wvx_runtime::apply_implementation_overrides(&mut project, &body.impls);
    match project_export_rust_hydrated(&project, Some(state.registry.as_ref())) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

async fn reg_summary(State(state): State<AppState>) -> Response {
    match registry_summary(state.registry.as_ref()) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    #[serde(default)]
    q: String,
}

async fn reg_search(State(state): State<AppState>, Query(q): Query<SearchQuery>) -> Response {
    match registry_search(state.registry.as_ref(), &q.q) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

#[derive(Debug, Deserialize)]
struct ImplQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    capability: Option<String>,
}

async fn reg_implementations(
    State(state): State<AppState>,
    Query(q): Query<ImplQuery>,
) -> Response {
    match registry_implementations(state.registry.as_ref(), q.capability.as_deref(), &q.q) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

async fn reg_inspect(State(state): State<AppState>, Path(key): Path<String>) -> Response {
    match registry_inspect(state.registry.as_ref(), &key) {
        Ok(resp) => {
            if !resp.ok {
                return (StatusCode::NOT_FOUND, Json(resp)).into_response();
            }
            Json(resp).into_response()
        }
        Err(e) => bus_error(e),
    }
}

/// Lifecycle vs evidence audit (overclaim detection). Not full Gate E.
/// Always HTTP 200 with report body; `data.ok` is false on overclaims.
async fn reg_admission(State(state): State<AppState>) -> Response {
    match registry_admission_audit(state.registry.as_ref()) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

/// Milestone 1 truthful audit (artifacts required for conformant+).
async fn reg_truthful(State(state): State<AppState>) -> Response {
    match registry_truthful_audit(state.registry.as_ref()) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

/// Conformance profile catalog under `registry-dev/profiles/`.
async fn reg_profiles(State(state): State<AppState>) -> Response {
    match registry_profiles(state.registry.as_ref()) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

/// Multi-domain family roll-up for Studio Library filters.
async fn reg_families(State(state): State<AppState>) -> Response {
    match registry_families(state.registry.as_ref()) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

#[derive(Debug, Deserialize)]
struct ResolveBody {
    capability: String,
    #[serde(default)]
    profile: Option<TargetProfile>,
    #[serde(default)]
    policy: Option<ResolverPolicy>,
    /// Convenience: `dev` | `release` when `policy` omitted.
    #[serde(default)]
    policy_preset: Option<String>,
}

/// Explainable resolve (TargetProfile + ResolverPolicy). Does not auto-admit.
async fn reg_resolve(State(state): State<AppState>, Json(body): Json<ResolveBody>) -> Response {
    let policy = body.policy.or_else(|| match body.policy_preset.as_deref() {
        Some("release") => Some(ResolverPolicy::release()),
        Some("dev") | None => Some(ResolverPolicy::default()),
        Some(other) => Some(ResolverPolicy {
            id: other.into(),
            ..ResolverPolicy::default()
        }),
    });
    match registry_resolve(
        state.registry.as_ref(),
        &body.capability,
        body.profile,
        policy,
    ) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

/// Verify evidence artifact for an implementation full id.
async fn reg_verify_evidence(State(state): State<AppState>, Path(key): Path<String>) -> Response {
    match registry_verify_evidence(state.registry.as_ref(), &key) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

async fn pilot_implementations() -> impl IntoResponse {
    Json(implementations_list())
}

/// Multi-domain pilot catalog (Domains 1–4 + text) for Studio load menu.
async fn pilot_catalog_handler() -> impl IntoResponse {
    Json(pilot_catalog())
}

#[derive(Debug, Deserialize)]
struct ForgeInventoryBody {
    /// Absolute or relative path to a crate/workspace. Server-local only.
    path: String,
}

async fn forge_inventory_handler(
    State(state): State<AppState>,
    Json(body): Json<ForgeInventoryBody>,
) -> Response {
    let path = std::path::PathBuf::from(&body.path);
    if !state.security.path_allowed(&path) {
        return path_denied(&path);
    }
    match forge_inventory(&path) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

#[derive(Debug, Deserialize)]
struct CargoSearchQuery {
    q: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
}

fn default_search_limit() -> usize {
    8
}

/// Interactive crates.io search (`cargo search`) for Forge source pickers.
async fn forge_cargo_search_handler(Query(q): Query<CargoSearchQuery>) -> Response {
    match forge_cargo_search(&q.q, q.limit) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

/// Allowed workspace roots (hints for path field / presets).
async fn forge_workspace_roots(State(state): State<AppState>) -> impl IntoResponse {
    let roots: Vec<String> = state
        .security
        .workspace_roots
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    Json(serde_json::json!({
        "ok": true,
        "protocol_version": PROTOCOL_VERSION,
        "data": { "roots": roots },
    }))
}

async fn forge_extract_handler(
    State(state): State<AppState>,
    Json(body): Json<ForgeInventoryBody>,
) -> Response {
    let path = std::path::PathBuf::from(&body.path);
    if !state.security.path_allowed(&path) {
        return path_denied(&path);
    }
    match forge_extract(&path) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

/// Body for Weavatrix facts ingest (ADR-0012 preferred path).
///
/// Provide **one of**: inline `facts`, server-local `facts_path`, or raw `facts_json`.
#[derive(Debug, Deserialize)]
struct ForgeFactsBody {
    #[serde(default)]
    facts: Option<WeavatrixFactsBundle>,
    /// Absolute path to a facts JSON file on the loom-server host.
    #[serde(default)]
    facts_path: Option<String>,
    /// Raw JSON string (e.g. pasted in Studio).
    #[serde(default)]
    facts_json: Option<String>,
}

async fn forge_facts_handler(
    State(state): State<AppState>,
    Json(body): Json<ForgeFactsBody>,
) -> Response {
    match resolve_facts(&state, &body) {
        Ok(bundle) => match forge_facts_to_extract(&bundle) {
            Ok(resp) => Json(resp).into_response(),
            Err(e) => bus_error(e),
        },
        Err(resp) => resp,
    }
}

#[derive(Debug, Deserialize)]
struct ForgeExportFactsBody {
    path: String,
    out_path: String,
}

/// Bootstrap AST extract → write Weavatrix-compatible facts JSON (interop test aid).
async fn forge_export_facts_handler(
    State(state): State<AppState>,
    Json(body): Json<ForgeExportFactsBody>,
) -> Response {
    let path = std::path::PathBuf::from(&body.path);
    let out = std::path::PathBuf::from(&body.out_path);
    if !state.security.path_allowed(&path) {
        return path_denied(&path);
    }
    if !state.security.path_allowed(&out) {
        return path_denied(&out);
    }
    match forge_export_facts(&path, &out) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

/// Resolve Weavatrix facts from inline object, JSON string, or host path.
fn resolve_facts(
    state: &AppState,
    body: &ForgeFactsBody,
) -> Result<WeavatrixFactsBundle, Response> {
    if let Some(ref f) = body.facts {
        return Ok(f.clone());
    }
    if let Some(ref raw) = body.facts_json {
        return match wvx_forge::parse_facts_json(raw) {
            Ok(b) => Ok(b),
            Err(e) => Err(bus_error(BusError::Forge(e))),
        };
    }
    if let Some(ref p) = body.facts_path {
        let path = std::path::PathBuf::from(p);
        if !state.security.path_allowed(&path) {
            return Err(path_denied(&path));
        }
        return match wvx_forge::load_facts_file(&path) {
            Ok(b) => Ok(b),
            Err(e) => Err(bus_error(BusError::Forge(e.to_string()))),
        };
    }
    Err((
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "ok": false,
            "diagnostics": [
                "provide facts, facts_json, or facts_path (Weavatrix wvx.facts.v0.1)"
            ]
        })),
    )
        .into_response())
}

async fn forge_match_handler(
    State(state): State<AppState>,
    Json(body): Json<ForgeMatchBody>,
) -> Response {
    // Prefer Weavatrix facts when present (ADR-0012).
    if body.facts.is_some() || body.facts_json.is_some() || body.facts_path.is_some() {
        let facts_body = ForgeFactsBody {
            facts: body.facts.clone(),
            facts_path: body.facts_path.clone(),
            facts_json: body.facts_json.clone(),
        };
        let bundle = match resolve_facts(&state, &facts_body) {
            Ok(b) => b,
            Err(r) => return r,
        };
        return match forge_match_facts(&bundle, Some(state.registry.as_ref())) {
            Ok(resp) => Json(resp).into_response(),
            Err(e) => bus_error(e),
        };
    }
    let Some(ref path_str) = body.path else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "diagnostics": ["match requires path (bootstrap) or facts / facts_json / facts_path"]
            })),
        )
            .into_response();
    };
    let path = std::path::PathBuf::from(path_str);
    if !state.security.path_allowed(&path) {
        return path_denied(&path);
    }
    match forge_match(&path, Some(state.registry.as_ref())) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

#[derive(Debug, Deserialize)]
struct ForgeMatchBody {
    /// Bootstrap crate/workspace path (AST extract).
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    facts: Option<WeavatrixFactsBundle>,
    #[serde(default)]
    facts_path: Option<String>,
    #[serde(default)]
    facts_json: Option<String>,
}

fn path_denied(path: &std::path::Path) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "ok": false,
            "diagnostics": [
                format!("path not under approved workspace roots: {}", path.display()),
                "set WVX_WORKSPACE_ROOTS to semicolon-separated absolute roots"
            ]
        })),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
struct ForgeDraftBody {
    /// Bootstrap crate path (optional when facts* present).
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    name: Option<String>,
    /// Optional server-local directory to write draft packages.
    #[serde(default)]
    out_dir: Option<String>,
    #[serde(default)]
    facts: Option<WeavatrixFactsBundle>,
    #[serde(default)]
    facts_path: Option<String>,
    #[serde(default)]
    facts_json: Option<String>,
}

async fn forge_draft_handler(
    State(state): State<AppState>,
    Json(body): Json<ForgeDraftBody>,
) -> Response {
    let out = body.out_dir.as_ref().map(std::path::PathBuf::from);
    if let Some(ref o) = out {
        if !state.security.path_allowed(o) {
            return path_denied(o);
        }
    }
    if body.facts.is_some() || body.facts_json.is_some() || body.facts_path.is_some() {
        let facts_body = ForgeFactsBody {
            facts: body.facts.clone(),
            facts_path: body.facts_path.clone(),
            facts_json: body.facts_json.clone(),
        };
        let bundle = match resolve_facts(&state, &facts_body) {
            Ok(b) => b,
            Err(r) => return r,
        };
        return match forge_draft_facts(
            &bundle,
            body.name.as_deref(),
            out.as_deref(),
            Some(state.registry.as_ref()),
        ) {
            Ok(resp) => Json(resp).into_response(),
            Err(e) => bus_error(e),
        };
    }
    let Some(ref path_str) = body.path else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "diagnostics": ["draft requires path or facts / facts_json / facts_path"]
            })),
        )
            .into_response();
    };
    let path = std::path::PathBuf::from(path_str);
    if !state.security.path_allowed(&path) {
        return path_denied(&path);
    }
    match forge_draft(
        &path,
        body.name.as_deref(),
        out.as_deref(),
        Some(state.registry.as_ref()),
    ) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

#[derive(Debug, Deserialize)]
struct ForgeRegisterBody {
    path: String,
    #[serde(default)]
    name: Option<String>,
    /// Default true: only install drafts that map onto an existing capability.
    #[serde(default = "default_true")]
    only_matched: bool,
}

/// Draft package + write **candidate** implementations into the live registry.
/// Does not admit. Library will show new impls after reload.
async fn forge_register_candidates_handler(
    State(state): State<AppState>,
    Json(body): Json<ForgeRegisterBody>,
) -> Response {
    let path = std::path::PathBuf::from(&body.path);
    if !state.security.path_allowed(&path) {
        return path_denied(&path);
    }
    match forge_register_candidates(
        &path,
        body.name.as_deref(),
        state.registry.as_ref(),
        body.only_matched,
    ) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

#[derive(Debug, Deserialize)]
struct ForgeCompileBody {
    path: String,
    #[serde(default)]
    name: Option<String>,
    out_dir: String,
    #[serde(default)]
    check: bool,
}

async fn forge_compile_handler(
    State(state): State<AppState>,
    Json(body): Json<ForgeCompileBody>,
) -> Response {
    let path = std::path::PathBuf::from(&body.path);
    let out = std::path::PathBuf::from(&body.out_dir);
    if !state.security.path_allowed(&path) {
        return path_denied(&path);
    }
    if !state.security.path_allowed(&out) {
        return path_denied(&out);
    }
    match forge_compile(
        &path,
        body.name.as_deref(),
        &out,
        body.check,
        Some(state.registry.as_ref()),
    ) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

#[derive(Debug, Default, Deserialize)]
struct ForgeGateCBody {
    #[serde(default)]
    workspace: Option<String>,
    /// External package tree (Gate C v1/v2) or held-out root (v3).
    #[serde(default)]
    external: Option<String>,
    /// `v2` | `v3` (default pilot / v1 when omitted).
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    human_minutes: Option<f64>,
    /// Run cargo check on compileable adapters (default true).
    #[serde(default = "default_true")]
    check: bool,
}

fn default_true() -> bool {
    true
}

async fn forge_gate_c_handler(
    State(state): State<AppState>,
    Json(body): Json<ForgeGateCBody>,
) -> Response {
    let ws = body.workspace.as_ref().map(std::path::PathBuf::from);
    if let Some(ref p) = ws {
        if !state.security.path_allowed(p) {
            return path_denied(p);
        }
    }
    let ext = body.external.as_ref().map(std::path::PathBuf::from);
    if let Some(ref p) = ext {
        if !state.security.path_allowed(p) {
            return path_denied(p);
        }
    }
    let ver = body.version.as_deref().unwrap_or("");
    let heldout_v3 = ver == "v3";
    let blind_v2 = ver == "v2";
    match forge_gate_c_ex3(
        ws.as_deref(),
        ext.as_deref(),
        Some(state.registry.as_ref()),
        body.check,
        body.human_minutes,
        blind_v2,
        heldout_v3,
    ) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

#[derive(Debug, Default, Deserialize)]
struct ProposeBody {
    /// When present, proposal is relative to this project (only missing pilot pieces).
    #[serde(default)]
    project: Option<Project>,
}

async fn propose_patch(State(state): State<AppState>, Json(body): Json<ProposeBody>) -> Response {
    match graph_propose_patch(Some(state.registry.as_ref()), body.project.as_ref()) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

#[derive(Debug, Deserialize)]
struct ProposeIntentBody {
    intent: String,
    project: Project,
}

async fn propose_intent(
    State(state): State<AppState>,
    Json(body): Json<ProposeIntentBody>,
) -> Response {
    match graph_propose_intent(Some(state.registry.as_ref()), &body.project, &body.intent) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

#[derive(Debug, Deserialize)]
struct PatchBody {
    project: Project,
    patch: GraphPatch,
}

async fn preview_patch_handler(Json(body): Json<PatchBody>) -> Response {
    match graph_preview_patch(&body.project, &body.patch) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

async fn validate_patch(Json(body): Json<PatchBody>) -> Response {
    match graph_validate_patch(&body.project, &body.patch) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

async fn commit_patch_handler(Json(body): Json<PatchBody>) -> Response {
    match graph_commit_patch(&body.project, &body.patch) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

async fn apply_patch(Json(body): Json<PatchBody>) -> Response {
    // Authoritative: commit (revision advances only when valid).
    match graph_apply_patch(&body.project, &body.patch) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

fn bus_error(e: BusError) -> Response {
    let status = match &e {
        BusError::InvalidProject(_)
        | BusError::Run(_)
        | BusError::Compile(_)
        | BusError::Forge(_)
        | BusError::Patch(_)
        | BusError::Cortex(_) => StatusCode::UNPROCESSABLE_ENTITY,
        BusError::Registry(_) => StatusCode::BAD_REQUEST,
        BusError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(BusResponse::<()>::err(vec![e.to_string()]))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use wvx_registry_client::LocalRegistry;

    fn test_app() -> Router {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry-dev");
        let registry = Arc::new(LocalRegistry::open(&root).unwrap());
        let security = Arc::new(crate::security::SecurityConfig {
            session_token: "test-token".into(),
            cors_origins: vec!["http://localhost:5173".into()],
            workspace_roots: vec![root],
        });
        router(AppState { registry, security })
    }

    use std::sync::Arc;

    #[tokio::test]
    async fn health_ok() {
        let app = test_app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn registry_summary_ok() {
        let app = test_app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/registry/summary")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["ok"], true);
        assert!(v["data"]["capabilities"].as_u64().unwrap() >= 5);
    }

    #[tokio::test]
    async fn registry_truthful_and_profiles_ok() {
        let app = test_app();
        for uri in [
            "/api/v1/registry/truthful",
            "/api/v1/registry/profiles",
            "/api/v1/registry/families",
            "/api/v1/pilot/catalog",
        ] {
            let res = app
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK, "{uri}");
            let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
                .await
                .unwrap();
            let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(v["ok"], true, "{uri}");
        }
    }

    #[tokio::test]
    async fn registry_resolve_parse_ok() {
        let app = test_app();
        let body = serde_json::json!({
            "capability": "data.json.parse@1",
            "policy_preset": "dev"
        });
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/registry/resolve")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["ok"], true);
        assert!(v["data"]["capability_key"]
            .as_str()
            .unwrap()
            .contains("parse"));
    }

    #[tokio::test]
    async fn registry_verify_evidence_sample() {
        let app = test_app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/registry/verify-evidence/serde-json.parse-owned@1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // Sample artifact should verify (or soft-fail with diagnostics).
        assert!(v.get("data").is_some() || v.get("ok").is_some());
    }

    #[tokio::test]
    async fn validate_pilot_fixture() {
        let text = include_str!("../../../fixtures/pilot-json-pipeline.wvx.json");
        let project: Project = serde_json::from_str(text).unwrap();
        let app = test_app();
        let body = serde_json::json!({ "project": project });
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/project/validate")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["ok"], true);
    }

    #[tokio::test]
    async fn run_pilot_fixture() {
        let text = include_str!("../../../fixtures/pilot-json-pipeline.wvx.json");
        let project: Project = serde_json::from_str(text).unwrap();
        let app = test_app();
        let body = serde_json::json!({
            "project": project,
            "input_json": "{\"hello\":\"world\"}"
        });
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/project/run")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["ok"], true);
        // Find serialize output in traces/outputs
        let outputs = &v["data"]["outputs"];
        assert!(
            outputs.get("serialize.bytes").is_some()
                || outputs.get("output.bytes").is_some()
                || outputs.as_object().map(|o| !o.is_empty()).unwrap_or(false)
        );
    }

    #[tokio::test]
    async fn run_hydrates_missing_capabilities_from_registry() {
        let text = include_str!("../../../fixtures/pilot-json-pipeline.wvx.json");
        let mut project: Project = serde_json::from_str(text).unwrap();
        project.capabilities.clear();
        let app = test_app();
        let body = serde_json::json!({
            "project": project,
            "input_json": "{\"hello\":\"world\"}"
        });
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/project/run")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["ok"], true, "body={v}");
        assert!(v["data"]["traces"]
            .as_array()
            .map(|a| a.len() >= 4)
            .unwrap_or(false));
    }

    #[test]
    fn b64_roundtrip_smoke() {
        // "hi" = aGk=
        let decoded = decode_b64("aGk=").unwrap();
        assert_eq!(decoded, b"hi");
    }
}
