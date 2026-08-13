//! Loom Forge — **thin semantic ingestion** toward the Registry (ADR-0012).
//!
//! Target pipeline:
//! ```text
//! Weavatrix (code facts) → Forge (classify / match) → Registry
//! ```
//!
//! **Bootstrap (current):** local Cargo inventory + AST/line extract live here so
//! the pilot works offline. They are **not** product positioning as a second
//! Weavatrix code graph. Deep indexing/search/impact belong in Weavatrix.
//!
//! **Keep in Loom:** capability matching, adapter drafts, compileable *semantic*
//! adapter packs, Gate C metrics over capability mapping.
//!
//! Does **not** run build.rs, proc macros, or network for inventory/extract.

mod capability_match;
mod compile_adapter;
mod draft;
mod economics;
mod extract;

pub use capability_match::{
    match_candidate, match_candidates, CapabilityMatch, CandidateMatchRow, MappingKind,
    MatchReport, OntologyCapability, OntologyPort,
};
pub use compile_adapter::{
    compile_adapter_from_draft, compile_adapters_batch, CompileAdapterReport, CompileBatchReport,
};
pub use draft::{
    draft_adapters, draft_adapters_with_ontology, draft_from_extract, write_draft_files,
    AdapterDraft, DraftReport,
};
pub use economics::{
    default_workspace_root, pilot_gate_c_expectations, pilot_ontology, run_gate_c_pilot,
    GateCCaseResult, GateCReport,
};
pub use extract::{extract_public_api, ApiCandidate, CandidateKind, CandidateShape, ExtractReport};

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ForgeError {
    #[error("path does not exist: {0}")]
    Missing(PathBuf),
    #[error("not a directory: {0}")]
    NotDir(PathBuf),
    #[error("io {0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("parse {0}: {1}")]
    Parse(PathBuf, String),
}

/// Discrete admission-adjacent status for an inventory pass (not a percentage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryStatus {
    InventoryOnly,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInventory {
    pub path: String,
    pub name: String,
    pub version: String,
    pub edition: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub publish: Option<bool>,
    pub is_workspace_root: bool,
    pub members: Vec<String>,
    pub dependencies: Vec<DepEntry>,
    pub dev_dependencies: Vec<DepEntry>,
    pub features: Vec<String>,
    pub lib_present: bool,
    pub bins: Vec<String>,
    pub risk_indicators: Vec<String>,
    pub status: InventoryStatus,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepEntry {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryReport {
    pub root: String,
    pub packages: Vec<PackageInventory>,
    pub scanned_at_unix: u64,
}

/// Inventory a crate directory or workspace root (static only).
pub fn inventory_path(root: impl AsRef<Path>) -> Result<InventoryReport, ForgeError> {
    let root = root.as_ref();
    if !root.exists() {
        return Err(ForgeError::Missing(root.to_path_buf()));
    }
    if !root.is_dir() {
        return Err(ForgeError::NotDir(root.to_path_buf()));
    }

    let mut packages = Vec::new();
    let manifest = root.join("Cargo.toml");
    if !manifest.is_file() {
        return Err(ForgeError::Missing(manifest));
    }

    let root_doc = read_toml(&manifest)?;
    if root_doc.get("workspace").is_some() {
        let members = workspace_members(&root_doc);
        if members.is_empty() {
            // Virtual or single — still inventory root if it has [package]
            if root_doc.get("package").is_some() {
                packages.push(inventory_package(root, &root_doc, true, &members)?);
            } else {
                packages.push(PackageInventory {
                    path: root.display().to_string(),
                    name: "(workspace)".into(),
                    version: "0.0.0".into(),
                    edition: None,
                    description: None,
                    license: None,
                    publish: Some(false),
                    is_workspace_root: true,
                    members: members.clone(),
                    dependencies: vec![],
                    dev_dependencies: vec![],
                    features: vec![],
                    lib_present: false,
                    bins: vec![],
                    risk_indicators: vec![],
                    status: InventoryStatus::InventoryOnly,
                    notes: vec!["virtual workspace root".into()],
                });
            }
        }
        for rel in &members {
            let member_path = root.join(rel);
            let member_manifest = member_path.join("Cargo.toml");
            if !member_manifest.is_file() {
                continue;
            }
            let doc = read_toml(&member_manifest)?;
            packages.push(inventory_package(&member_path, &doc, false, &[])?);
        }
        if packages.is_empty() && root_doc.get("package").is_some() {
            packages.push(inventory_package(root, &root_doc, true, &members)?);
        }
    } else {
        packages.push(inventory_package(root, &root_doc, false, &[])?);
    }

    packages.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(InventoryReport {
        root: root.display().to_string(),
        packages,
        scanned_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    })
}

fn inventory_package(
    path: &Path,
    doc: &toml::Value,
    is_workspace_root: bool,
    members: &[String],
) -> Result<PackageInventory, ForgeError> {
    let pkg = doc.get("package").and_then(|v| v.as_table());
    let name = pkg
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("(unknown)")
        .to_string();
    let version = pkg
        .and_then(|p| p.get("version"))
        .map(|v| {
            if let Some(s) = v.as_str() {
                s.to_string()
            } else if v.get("workspace").and_then(|w| w.as_bool()) == Some(true) {
                "workspace".into()
            } else {
                "0.0.0".into()
            }
        })
        .unwrap_or_else(|| "0.0.0".into());
    let edition = pkg.and_then(|p| p.get("edition")).and_then(|v| {
        v.as_str().map(str::to_string).or_else(|| {
            if v.get("workspace").and_then(|w| w.as_bool()) == Some(true) {
                Some("workspace".into())
            } else {
                None
            }
        })
    });
    let description = pkg
        .and_then(|p| p.get("description"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let license = pkg
        .and_then(|p| p.get("license"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let publish = pkg.and_then(|p| p.get("publish")).and_then(|v| {
        if let Some(b) = v.as_bool() {
            Some(b)
        } else if v.as_array().is_some() {
            Some(true)
        } else {
            None
        }
    });

    let dependencies = deps_table(doc.get("dependencies"));
    let dev_dependencies = deps_table(doc.get("dev-dependencies"));
    let features = doc
        .get("features")
        .and_then(|v| v.as_table())
        .map(|t| t.keys().cloned().collect())
        .unwrap_or_default();

    let lib_present = path.join("src/lib.rs").is_file()
        || doc.get("lib").is_some();
    let mut bins = Vec::new();
    if path.join("src/main.rs").is_file() {
        bins.push(name.clone());
    }
    if let Some(arr) = doc.get("bin").and_then(|v| v.as_array()) {
        for b in arr {
            if let Some(n) = b.get("name").and_then(|v| v.as_str()) {
                bins.push(n.to_string());
            }
        }
    }

    let risk = risk_indicators(path, doc);
    let mut notes = Vec::new();
    if risk.iter().any(|r| r == "build.rs") {
        notes.push("has build.rs — not executed in static inventory".into());
    }
    if risk.iter().any(|r| r == "proc-macro") {
        notes.push("proc-macro package — elevated review".into());
    }

    Ok(PackageInventory {
        path: path.display().to_string(),
        name,
        version,
        edition,
        description,
        license,
        publish,
        is_workspace_root,
        members: members.to_vec(),
        dependencies,
        dev_dependencies,
        features,
        lib_present,
        bins,
        risk_indicators: risk,
        status: InventoryStatus::InventoryOnly,
        notes,
    })
}

fn risk_indicators(path: &Path, doc: &toml::Value) -> Vec<String> {
    let mut out = Vec::new();
    if path.join("build.rs").is_file() {
        out.push("build.rs".into());
    }
    if doc
        .get("lib")
        .and_then(|l| l.get("proc-macro"))
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        out.push("proc-macro".into());
    }
    // cheap scan of lib.rs for unsafe / ffi hints (bounded)
    for rel in ["src/lib.rs", "src/main.rs"] {
        let p = path.join(rel);
        if let Ok(text) = fs::read_to_string(&p) {
            let sample = if text.len() > 200_000 {
                &text[..200_000]
            } else {
                &text
            };
            if sample.contains("unsafe ") || sample.contains("unsafe{") {
                out.push("unsafe".into());
            }
            if sample.contains("extern \"C\"") || sample.contains("libc::") {
                out.push("FFI".into());
            }
            break;
        }
    }
    if path.join("Cargo.lock").is_file() {
        // not a risk, skip
    }
    out.sort();
    out.dedup();
    out
}

fn deps_table(value: Option<&toml::Value>) -> Vec<DepEntry> {
    let Some(table) = value.and_then(|v| v.as_table()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, spec) in table {
        let source = if let Some(s) = spec.as_str() {
            format!("version={s}")
        } else if let Some(t) = spec.as_table() {
            if let Some(p) = t.get("path").and_then(|v| v.as_str()) {
                format!("path={p}")
            } else if let Some(g) = t.get("git").and_then(|v| v.as_str()) {
                format!("git={g}")
            } else if let Some(v) = t.get("version").and_then(|v| v.as_str()) {
                format!("version={v}")
            } else if t.get("workspace").and_then(|v| v.as_bool()) == Some(true) {
                "workspace=true".into()
            } else {
                "table".into()
            }
        } else {
            "unknown".into()
        };
        out.push(DepEntry {
            name: name.clone(),
            source,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn workspace_members(doc: &toml::Value) -> Vec<String> {
    doc.get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .filter(|s| !s.contains('*')) // skip globs in v0.1
                .collect()
        })
        .unwrap_or_default()
}

fn read_toml(path: &Path) -> Result<toml::Value, ForgeError> {
    let text = fs::read_to_string(path).map_err(|e| ForgeError::Io(path.to_path_buf(), e))?;
    toml::from_str(&text).map_err(|e| ForgeError::Parse(path.to_path_buf(), e.to_string()))
}

/// JSON-serializable summary map for HTTP.
pub fn report_to_json(report: &InventoryReport) -> serde_json::Value {
    serde_json::to_value(report).unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_self_workspace() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let report = inventory_path(&root).unwrap();
        assert!(!report.packages.is_empty());
        assert!(report.packages.iter().any(|p| p.name == "wvx-ir"));
        assert!(report.packages.iter().any(|p| p.name == "loom-server"));
    }
}
