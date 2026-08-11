//! Compile a validated WVX project to a readable, cargo-buildable Rust package.
//!
//! Production adapters live in the external `wvx-adapters` crate and are
//! **vendored** into each export under `vendor/wvx-adapters` so the package is
//! self-contained.

mod adapters;
mod emit;
mod order;
mod vendor;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;
use thiserror::Error;
use wvx_ir::Project;
use wvx_validator::validate_project;

pub use adapters::{default_implementation, known_implementation_ids};

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("project is not valid: {0}")]
    InvalidProject(String),
    #[error("unsupported implementation `{0}` for capability `{1}`")]
    UnsupportedImplementation(String, String),
    #[error("graph error: {0}")]
    Graph(String),
    #[error("io: {0}")]
    Io(String),
    #[error("cargo failed: {0}")]
    Cargo(String),
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

/// Generate a Cargo package for the project (in memory).
pub fn compile_to_rust(project: &Project) -> Result<GeneratedWorkspace, CompileError> {
    let report = validate_project(project);
    if !report.is_ok() {
        let msg = report
            .errors()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(CompileError::InvalidProject(msg));
    }

    let order = order::topo_order(project).map_err(CompileError::Graph)?;
    let resolved = resolve_implementations(project)?;
    let needed_impls: BTreeSet<String> = resolved.values().cloned().collect();

    let pkg = sanitize_pkg_name(&project.id);
    let mut files = Vec::new();

    files.push(GeneratedFile {
        relative_path: "Cargo.toml".into(),
        contents: emit::cargo_toml(&pkg, &needed_impls),
    });

    files.push(GeneratedFile {
        relative_path: "weavatrix.lock".into(),
        contents: emit::lockfile(project, &resolved),
    });

    if adapters::needs_external_adapters(&needed_impls) {
        files.extend(vendor::vendor_adapters_files()?);
    }

    files.push(GeneratedFile {
        relative_path: "src/generated_pipeline.rs".into(),
        contents: emit::pipeline(project, &order, &resolved)?,
    });

    files.push(GeneratedFile {
        relative_path: "src/main.rs".into(),
        contents: emit::main_rs(),
    });

    files.push(GeneratedFile {
        relative_path: "src/lib.rs".into(),
        contents: "//! Generated Loom export.\n//!\n//! Uses external crate `wvx-adapters` (vendored under `vendor/`).\n\npub mod generated_pipeline;\n\npub use generated_pipeline::run_pipeline;\n".into(),
    });

    Ok(GeneratedWorkspace {
        files,
        package_name: pkg,
    })
}

/// Compile, write to `out_dir`, optionally `cargo check` / `cargo run`.
pub fn export_to_directory(
    project: &Project,
    out_dir: &Path,
    check: bool,
    run_input: Option<&[u8]>,
) -> Result<ExportReport, CompileError> {
    let ws = compile_to_rust(project)?;
    if out_dir.exists() {
        // Only remove if it looks like a previous export (has weavatrix.lock).
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

    let mut report = ExportReport {
        package_name: ws.package_name,
        out_dir: out_dir.display().to_string(),
        files: ws.files.len(),
        check_ok: None,
        run_stdout: None,
    };

    if check || run_input.is_some() {
        let status = Command::new("cargo")
            .arg("check")
            .arg("--quiet")
            .current_dir(out_dir)
            .status()
            .map_err(|e| CompileError::Cargo(e.to_string()))?;
        report.check_ok = Some(status.success());
        if !status.success() {
            return Err(CompileError::Cargo(
                "cargo check failed for exported project".into(),
            ));
        }
    }

    if let Some(input) = run_input {
        let output = Command::new("cargo")
            .arg("run")
            .arg("--quiet")
            .env(
                "WVX_PIPELINE_INPUT",
                String::from_utf8_lossy(input).as_ref(),
            )
            .current_dir(out_dir)
            .output()
            .map_err(|e| CompileError::Cargo(e.to_string()))?;
        if !output.status.success() {
            return Err(CompileError::Cargo(format!(
                "cargo run failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        report.run_stdout = Some(String::from_utf8_lossy(&output.stdout).into_owned());
    }

    Ok(report)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportReport {
    pub package_name: String,
    pub out_dir: String,
    pub files: usize,
    pub check_ok: Option<bool>,
    pub run_stdout: Option<String>,
}

/// instance_id → implementation_id
fn resolve_implementations(project: &Project) -> Result<BTreeMap<String, String>, CompileError> {
    let mut map = BTreeMap::new();
    for instance in &project.instances {
        let cap = instance.capability.as_key();
        let impl_id = instance
            .implementation
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| default_implementation(&cap).map(str::to_string))
            .ok_or_else(|| {
                CompileError::UnsupportedImplementation("(none)".into(), cap.clone())
            })?;
        if !adapters::supports(&impl_id, &cap) {
            // I/O may use synthetic impls
            if !adapters::is_passthrough_io(&cap) {
                return Err(CompileError::UnsupportedImplementation(impl_id, cap));
            }
        }
        map.insert(instance.id.clone(), impl_id);
    }
    Ok(map)
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

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::PROJECT_SCHEMA_VERSION;

    #[test]
    fn compiles_pilot_fixture() {
        let text = include_str!("../../../fixtures/pilot-json-pipeline.wvx.json");
        let project: Project = serde_json::from_str(text).unwrap();
        assert_eq!(project.schema_version, PROJECT_SCHEMA_VERSION);
        let ws = compile_to_rust(&project).unwrap();
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
        assert!(pipeline.contents.contains("wvx_adapters::serde_json_parse_owned"));
        assert!(!pipeline.contents.contains("not yet linked"));
        assert!(ws
            .files
            .iter()
            .any(|f| f.relative_path.starts_with("vendor/wvx-adapters/")));
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
        let _ = fs::remove_dir_all(&dir);
    }
}
