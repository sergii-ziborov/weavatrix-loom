//! JSON routes mapped 1:1 onto command-bus operations.

use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use wvx_command_bus::{
    forge_extract, forge_inventory, graph_apply_patch, graph_propose_patch, implementations_list,
    project_export_rust_hydrated, project_run_hydrated, project_validate_hydrated,
    graph_propose_intent, registry_admission_audit, registry_implementations, registry_inspect,
    registry_search, registry_summary, BusError, BusResponse, PROTOCOL_VERSION,
};
use wvx_ir::Project;
use wvx_project_graph::GraphPatch;

use crate::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/protocol", get(protocol))
        .route("/api/v1/project/validate", post(validate_project))
        .route("/api/v1/project/run", post(run_project))
        .route("/api/v1/project/export-rust", post(export_rust))
        .route("/api/v1/registry/summary", get(reg_summary))
        .route("/api/v1/registry/search", get(reg_search))
        .route("/api/v1/registry/implementations", get(reg_implementations))
        .route("/api/v1/registry/inspect/{key}", get(reg_inspect))
        .route("/api/v1/registry/admission", get(reg_admission))
        .route("/api/v1/pilot/implementations", get(pilot_implementations))
        .route("/api/v1/forge/inventory", post(forge_inventory_handler))
        .route("/api/v1/forge/extract", post(forge_extract_handler))
        .route("/api/v1/graph/propose_patch", post(propose_patch))
        .route("/api/v1/graph/propose_intent", post(propose_intent))
        .route("/api/v1/graph/validate_patch", post(validate_patch))
        .route("/api/v1/graph/apply_patch", post(apply_patch))
        .with_state(state)
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

async fn reg_search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Response {
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
    match registry_implementations(
        state.registry.as_ref(),
        q.capability.as_deref(),
        &q.q,
    ) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

async fn reg_inspect(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Response {
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

async fn pilot_implementations() -> impl IntoResponse {
    Json(implementations_list())
}

#[derive(Debug, Deserialize)]
struct ForgeInventoryBody {
    /// Absolute or relative path to a crate/workspace. Server-local only.
    path: String,
}

async fn forge_inventory_handler(Json(body): Json<ForgeInventoryBody>) -> Response {
    let path = std::path::PathBuf::from(&body.path);
    match forge_inventory(&path) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

async fn forge_extract_handler(Json(body): Json<ForgeInventoryBody>) -> Response {
    let path = std::path::PathBuf::from(&body.path);
    match forge_extract(&path) {
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

async fn propose_patch(
    State(state): State<AppState>,
    Json(body): Json<ProposeBody>,
) -> Response {
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
    match graph_propose_intent(
        Some(state.registry.as_ref()),
        &body.project,
        &body.intent,
    ) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

#[derive(Debug, Deserialize)]
struct PatchBody {
    project: Project,
    patch: GraphPatch,
}

async fn validate_patch(Json(body): Json<PatchBody>) -> Response {
    match graph_apply_patch(&body.project, &body.patch) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => bus_error(e),
    }
}

async fn apply_patch(Json(body): Json<PatchBody>) -> Response {
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
    (
        status,
        Json(BusResponse::<()>::err(vec![e.to_string()])),
    )
        .into_response()
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
        let registry = Arc::new(LocalRegistry::open(root).unwrap());
        router(AppState { registry })
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
        assert!(outputs.get("serialize.bytes").is_some() || outputs.get("output.bytes").is_some() || outputs.as_object().map(|o| !o.is_empty()).unwrap_or(false));
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
        assert!(v["data"]["traces"].as_array().map(|a| a.len() >= 4).unwrap_or(false));
    }

    #[test]
    fn b64_roundtrip_smoke() {
        // "hi" = aGk=
        let decoded = decode_b64("aGk=").unwrap();
        assert_eq!(decoded, b"hi");
    }
}
