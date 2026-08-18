//! Workspace / source-tree discovery for digest recomputation.

use std::path::{Path, PathBuf};
use wvx_ir::Implementation;

use crate::evidence_artifact::{hash_tree, sha256_hex};

/// Walk up from `start` looking for a Cargo workspace (`Cargo.lock` + `crates/`).
pub fn discover_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    for _ in 0..8 {
        if dir.join("Cargo.lock").is_file() && dir.join("crates").is_dir() {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Best-effort workspace root: registry parent, then `CARGO_MANIFEST_DIR` walk.
pub fn workspace_root_near(registry_root: &Path) -> Option<PathBuf> {
    if let Some(w) = discover_workspace_root(registry_root) {
        return Some(w);
    }
    if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
        return discover_workspace_root(Path::new(&m));
    }
    None
}

/// Adapter crate directory from `sdk.emit.crate_path` or `crates/<crate_name>`.
pub fn adapter_crate_dir(workspace: &Path, imp: &Implementation) -> Option<PathBuf> {
    if let Some(rel) = imp
        .sdk
        .as_ref()
        .and_then(|s| s.emit.as_ref())
        .and_then(|e| e.crate_path.as_deref())
    {
        let p = workspace.join(rel);
        if p.is_dir() {
            return Some(p);
        }
    }
    if let Some(name) = imp.adapter.as_ref().map(|a| a.crate_name.as_str()) {
        let p = workspace.join("crates").join(name);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

/// Hash adapter crate `src/` plus emit template (source closure).
pub fn adapter_source_closure_digest(workspace: &Path, imp: &Implementation) -> String {
    let template = imp
        .sdk
        .as_ref()
        .and_then(|s| s.emit.as_ref())
        .map(|e| e.template.as_str())
        .unwrap_or("");
    let crate_name = imp
        .sdk
        .as_ref()
        .and_then(|s| s.emit.as_ref())
        .map(|e| e.crate_name.as_str())
        .unwrap_or("");
    let crate_path = imp
        .sdk
        .as_ref()
        .and_then(|s| s.emit.as_ref())
        .and_then(|e| e.crate_path.as_deref())
        .unwrap_or("");

    let mut payload = format!("closure|{crate_name}|{crate_path}|{template}|");
    if let Some(dir) = adapter_crate_dir(workspace, imp) {
        let src = dir.join("src");
        if src.is_dir() {
            if let Ok(h) = hash_tree(&src) {
                payload.push_str(&h);
            }
        }
        let cargo = dir.join("Cargo.toml");
        if cargo.is_file() {
            if let Ok(bytes) = std::fs::read(&cargo) {
                payload.push_str(&sha256_hex(&bytes));
            }
        }
    }
    sha256_hex(payload.as_bytes())
}

/// SHA-256 of the adapter crate `Cargo.toml` (package checksum).
pub fn package_checksum(workspace: &Path, imp: &Implementation) -> String {
    if let Some(dir) = adapter_crate_dir(workspace, imp) {
        let cargo = dir.join("Cargo.toml");
        if cargo.is_file() {
            if let Ok(bytes) = std::fs::read(&cargo) {
                return sha256_hex(&bytes);
            }
        }
    }
    sha256_hex(
        format!(
            "pkg|{}|{}@{}",
            imp.source.kind, imp.source.package, imp.source.package_version
        )
        .as_bytes(),
    )
}

/// Digest of `source_ref.revision` when present.
pub fn source_ref_revision_digest(imp: &Implementation) -> String {
    match imp
        .source_ref
        .as_ref()
        .and_then(|s| s.revision.as_deref())
        .map(str::trim)
    {
        Some(r) if !r.is_empty() => sha256_hex(r.as_bytes()),
        _ => "sha256:absent".into(),
    }
}

/// Implementation source tree: adapter crate directory when available.
pub fn implementation_source_tree_digest(workspace: &Path, imp: &Implementation) -> String {
    if let Some(dir) = adapter_crate_dir(workspace, imp) {
        if let Ok(h) = hash_tree(&dir) {
            return h;
        }
    }
    sha256_hex(
        format!(
            "tree|{}|{}|{}",
            imp.full_id(),
            imp.source.package,
            imp.source.package_version
        )
        .as_bytes(),
    )
}

pub fn cargo_lock_digest(workspace: &Path) -> String {
    let lock = workspace.join("Cargo.lock");
    if lock.is_file() {
        if let Ok(bytes) = std::fs::read(&lock) {
            return sha256_hex(&bytes);
        }
    }
    "sha256:absent".into()
}
