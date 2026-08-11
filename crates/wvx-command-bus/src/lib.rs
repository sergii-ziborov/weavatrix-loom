//! Single semantic API used by CLI, MCP, and future HTTP hosts.
//!
//! Hosts must not re-implement validation or graph rules.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use std::path::Path;
use wvx_compiler_rust::{
    compile_to_rust, export_to_directory, ExportReport, GeneratedWorkspace,
};
use wvx_ir::Project;
use wvx_registry_client::{LocalRegistry, RegistryError};
use std::collections::BTreeMap;
use wvx_runtime::{
    apply_implementation_overrides, list_pilot_implementations, run_project, HandlerRegistry,
    RunResult, RuntimeError, WvxValueMap,
};
use wvx_types::WvxValue;
use wvx_validator::{validate_project, ValidationReport};

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

/// Validate a project document.
pub fn project_validate(project: &Project) -> BusResponse<ValidationReport> {
    let report = validate_project(project);
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
    match compile_to_rust(project) {
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
    match export_to_directory(project, out_dir, check, run_input) {
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
    let mut project = project.clone();
    apply_implementation_overrides(&mut project, impl_overrides);
    let handlers = HandlerRegistry::with_pilot();
    let mut seed = WvxValueMap::new();
    seed.insert("bytes".into(), WvxValue::Bytes(input_bytes));
    let result = run_project(&project, &handlers, seed)?;
    Ok(BusResponse::ok(result))
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

/// Search capability ids in a local registry by substring (case-insensitive).
pub fn registry_search(
    registry: &LocalRegistry,
    query: &str,
) -> Result<BusResponse<Vec<String>>, BusError> {
    let q = query.trim().to_ascii_lowercase();
    let mut ids: Vec<String> = registry
        .list_capabilities()?
        .into_iter()
        .filter(|c| {
            if q.is_empty() {
                true
            } else {
                c.id.to_ascii_lowercase().contains(&q)
                    || c.kind.to_ascii_lowercase().contains(&q)
            }
        })
        .map(|c| format!("{}@{}", c.id, c.version))
        .collect();
    ids.sort();
    ids.dedup();
    Ok(BusResponse::ok(ids))
}

/// Load a project JSON file from disk.
pub fn load_project_path(path: &std::path::Path) -> Result<Project, BusError> {
    let text = std::fs::read_to_string(path).map_err(|e| BusError::Io(e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| BusError::Io(e.to_string()))
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
