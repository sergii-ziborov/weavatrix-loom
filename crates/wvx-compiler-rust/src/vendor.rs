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
        // Host-only registration is not needed in static export packages.
        if name == "register.rs" {
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
        let mut contents =
            fs::read_to_string(&path).map_err(|e| CompileError::Io(e.to_string()))?;
        if rel_str == "src/lib.rs" {
            contents = strip_host_registration_from_lib(&contents);
        }
        out.push(GeneratedFile {
            relative_path: format!("vendor/wvx-adapters/{rel_str}"),
            contents,
        });
    }
    Ok(())
}

/// Drop host/SDK registration surface from vendored adapters lib.
fn strip_host_registration_from_lib(src: &str) -> String {
    let mut out = String::new();
    let mut skip = false;
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("#[cfg(feature = \"host\")]") {
            skip = true;
            continue;
        }
        if skip {
            // Skip the next item (mod register; / pub use register::...).
            skip = false;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn standalone_adapters_cargo_toml() -> String {
    // Export-only: pure transform modules, no host/SDK registration feature.
    r#"[package]
name = "wvx-adapters"
version = "0.1.0"
edition = "2021"
publish = false
description = "Vendored Loom pilot adapters"

[features]
# Declared so vendored `#[cfg(feature = "host")]` is valid; never enabled on export.
host = []

[dependencies]
serde_json = "1"
json = "0.12"
# Domain 2 hashing + Domain 3 compression (vendored pilot modules)
sha2 = "0.10"
blake3 = "1.5"
flate2 = "1"
"#
    .into()
}

/// Vendor an arbitrary adapter crate (Gate F) into `vendor/<crate_name>/`.
///
/// `src_path` is relative to the monorepo (e.g. `crates/wvx-adapter-external-demo`)
/// or absolute. Rewrites workspace-inherited Cargo.toml to a standalone package.
pub fn vendor_crate_files(
    src_path: &str,
    crate_name: &str,
) -> Result<Vec<GeneratedFile>, CompileError> {
    let root = if Path::new(src_path).is_absolute() {
        PathBuf::from(src_path)
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(src_path)
    };
    let root = root
        .canonicalize()
        .map_err(|e| CompileError::Io(format!("vendor {src_path}: {e}")))?;
    if !root.is_dir() {
        return Err(CompileError::Io(format!(
            "sdk crate path missing: {}",
            root.display()
        )));
    }
    let mut files = Vec::new();
    walk_crate(&root, &root, crate_name, &mut files)?;
    for f in &mut files {
        if f.relative_path.ends_with("Cargo.toml") {
            f.contents = standalone_sdk_cargo_toml(crate_name, &f.contents);
        }
    }
    Ok(files)
}

fn walk_crate(
    root: &Path,
    dir: &Path,
    crate_name: &str,
    out: &mut Vec<GeneratedFile>,
) -> Result<(), CompileError> {
    let entries = fs::read_dir(dir).map_err(|e| CompileError::Io(e.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|e| CompileError::Io(e.to_string()))?;
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name == "target" || name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk_crate(root, &path, crate_name, out)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|e| CompileError::Io(e.to_string()))?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let contents = fs::read_to_string(&path).map_err(|e| CompileError::Io(e.to_string()))?;
        out.push(GeneratedFile {
            relative_path: format!("vendor/{crate_name}/{rel_str}"),
            contents,
        });
    }
    Ok(())
}

fn standalone_sdk_cargo_toml(crate_name: &str, _original: &str) -> String {
    // Export only needs pure transform functions (no SDK host feature).
    format!(
        r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"
publish = false
description = "Vendored Gate F adapter"

[features]
host = []

[dependencies]
serde_json = "1"
"#
    )
}
