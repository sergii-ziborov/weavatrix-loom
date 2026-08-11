//! Single semantic API used by CLI, MCP, and future HTTP hosts.
//!
//! Hosts must not re-implement validation or graph rules.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use std::path::Path;
use wvx_compiler_rust::{
    compile_to_rust, export_to_directory, ExportReport, GeneratedWorkspace,
};
use wvx_forge::{inventory_path, ForgeError, InventoryReport};
use wvx_ir::Project;
use wvx_registry_client::{
    CapabilityHit, ImplementationHit, LocalRegistry, RegistryError, RegistrySummary,
};
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
    #[error("forge: {0}")]
    Forge(String),
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
            .map(|i| ImplementationHit {
                full_id: i.full_id(),
                capability: i.capability.as_key(),
                source_kind: i.source.kind,
                package: i.source.package,
                adapter: i.adapter.map(|a| a.crate_name),
            })
            .filter(|h| {
                let q = query.trim().to_ascii_lowercase();
                q.is_empty()
                    || h.full_id.to_ascii_lowercase().contains(&q)
                    || h.package.to_ascii_lowercase().contains(&q)
            })
            .collect()
    } else {
        registry.search_implementations(query)?
    };
    Ok(BusResponse::ok(hits))
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
