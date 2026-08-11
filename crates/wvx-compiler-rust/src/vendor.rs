//! Vendor the external `wvx-adapters` crate into an export tree.

use crate::{CompileError, GeneratedFile};
use std::fs;
use std::path::{Path, PathBuf};

/// Absolute path to monorepo `crates/wvx-adapters`.
pub fn adapters_crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../wvx-adapters")
}

/// Collect all files under `wvx-adapters` as `vendor/wvx-adapters/...` entries.
pub fn vendor_adapters_files() -> Result<Vec<GeneratedFile>, CompileError> {
    let root = adapters_crate_dir();
    if !root.is_dir() {
        return Err(CompileError::Io(format!(
            "wvx-adapters crate missing at {}",
            root.display()
        )));
    }
    let mut files = Vec::new();
    walk(&root, &root, &mut files)?;
    // Rewrite Cargo.toml for standalone export (no workspace inheritance).
    for f in &mut files {
        if f.relative_path == "vendor/wvx-adapters/Cargo.toml" {
            f.contents = standalone_adapters_cargo_toml();
        }
    }
    Ok(files)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<GeneratedFile>) -> Result<(), CompileError> {
    let entries = fs::read_dir(dir).map_err(|e| CompileError::Io(e.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|e| CompileError::Io(e.to_string()))?;
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name == "target" || name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|e| CompileError::Io(e.to_string()))?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let contents = fs::read_to_string(&path).map_err(|e| CompileError::Io(e.to_string()))?;
        out.push(GeneratedFile {
            relative_path: format!("vendor/wvx-adapters/{rel_str}"),
            contents,
        });
    }
    Ok(())
}

fn standalone_adapters_cargo_toml() -> String {
    r#"[package]
name = "wvx-adapters"
version = "0.1.0"
edition = "2021"
publish = false
description = "Vendored Loom pilot adapters"

[dependencies]
serde_json = "1"
"#
    .into()
}
