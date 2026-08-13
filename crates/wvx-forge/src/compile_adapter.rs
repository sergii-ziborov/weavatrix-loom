//! FORGE-008: generate a **compileable** adapter crate from a draft.
//!
//! Semantic adapter packaging for a *single* composition path (Loom).
//! Broader workspace/CI/deploy scaffolding is **Realforge** (ADR-0012), not here.
//!
//! Static only until optional `cargo check`. Never sets evidence pass or admits.
//! Successful check may lift draft status label to `candidate` (local report only).

use crate::draft::AdapterDraft;
use crate::ForgeError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileAdapterReport {
    pub implementation_id: String,
    pub crate_name: String,
    pub out_dir: String,
    pub capability_key: String,
    pub mapping_kind: String,
    /// `inventory_only` always written; `candidate` only if cargo check succeeded.
    pub status: String,
    pub compile_ok: Option<bool>,
    pub compile_log: Option<String>,
    pub files_written: usize,
    /// SDK emit template for Gate F compiler (when shape known).
    pub sdk_emit_template: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileBatchReport {
    pub package_root: String,
    pub out_root: String,
    pub adapters: Vec<CompileAdapterReport>,
    pub compile_rate: f64,
    pub notes: Vec<String>,
}

/// Write a compileable adapter package for one draft under `out_dir`.
///
/// `package_root` is the scanned crate (path dependency target).
pub fn compile_adapter_from_draft(
    package_root: &Path,
    package_name: &str,
    draft: &AdapterDraft,
    out_dir: &Path,
    run_cargo_check: bool,
) -> Result<CompileAdapterReport, ForgeError> {
    let crate_name = sanitize_crate_name(&format!(
        "wvx_forge_{}",
        draft.implementation_id.replace('@', "_")
    ));
    let root = out_dir.join(&crate_name);
    fs::create_dir_all(root.join("src")).map_err(|e| ForgeError::Io(root.clone(), e))?;

    let package_root_str = cargo_path_dep(package_root);

    let call_path = upstream_call_path(package_name, draft);
    let shape = classify_shape(draft);
    let (lib_rs, sdk_template) = render_adapter_lib(
        package_name,
        draft,
        &call_path,
        shape,
    );
    let cargo_toml = render_cargo_toml(&crate_name, package_name, &package_root_str, shape);

    write(&root.join("Cargo.toml"), &cargo_toml)?;
    write(&root.join("src/lib.rs"), &lib_rs)?;
    write(
        &root.join("implementation.json"),
        &draft.implementation_json,
    )?;
    write(&root.join("capability.json"), &draft.capability_json)?;
    write(
        &root.join("README.md"),
        &format!(
            "# {} — Forge compileable adapter (FORGE-008)\n\n\
             Status written as inventory_only; cargo check may report candidate.\n\n\
             Upstream: `{}`\nCapability: `{}`\nMapping: {} (score {})\n",
            draft.implementation_id,
            call_path,
            draft.capability_id,
            draft.mapping_kind,
            draft.mapping_score
        ),
    )?;

    let mut notes = vec![
        "FORGE-008 compileable adapter — not admitted; evidence axes remain absent.".into(),
        format!("upstream_call={call_path}"),
    ];
    let mut compile_ok = None;
    let mut compile_log = None;
    let mut status = "inventory_only".to_string();

    if run_cargo_check {
        let output = Command::new("cargo")
            .arg("check")
            .arg("--quiet")
            .arg("--manifest-path")
            .arg(root.join("Cargo.toml"))
            .output()
            .map_err(|e| ForgeError::Io(root.clone(), e))?;
        let ok = output.status.success();
        compile_ok = Some(ok);
        let log = format!(
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        compile_log = Some(log.chars().take(4000).collect());
        if ok {
            status = "candidate".into();
            notes.push("cargo check passed → local status candidate (not registry admit).".into());
        } else {
            notes.push("cargo check failed — keep inventory_only; fix path/signature manually.".into());
        }
    } else {
        notes.push("cargo check skipped (pass check=true to verify compile).".into());
    }

    Ok(CompileAdapterReport {
        implementation_id: draft.implementation_id.clone(),
        crate_name,
        out_dir: root.display().to_string(),
        capability_key: draft.capability_id.clone(),
        mapping_kind: draft.mapping_kind.clone(),
        status,
        compile_ok,
        compile_log,
        files_written: 5,
        sdk_emit_template: sdk_template,
        notes,
    })
}

/// Compile adapters for all drafts that look like simple transforms.
pub fn compile_adapters_batch(
    package_root: &Path,
    package_name: &str,
    drafts: &[AdapterDraft],
    out_root: &Path,
    run_cargo_check: bool,
    only_reused_or_known_shape: bool,
) -> Result<CompileBatchReport, ForgeError> {
    fs::create_dir_all(out_root).map_err(|e| ForgeError::Io(out_root.to_path_buf(), e))?;
    let mut adapters = Vec::new();
    for d in drafts {
        if only_reused_or_known_shape {
            let shape = classify_shape(d);
            if shape == AdapterShape::Unknown
                && d.mapping_kind != "exact_shape"
                && d.mapping_kind != "compatible_shape"
            {
                continue;
            }
        }
        match compile_adapter_from_draft(package_root, package_name, d, out_root, run_cargo_check) {
            Ok(r) => adapters.push(r),
            Err(e) => adapters.push(CompileAdapterReport {
                implementation_id: d.implementation_id.clone(),
                crate_name: String::new(),
                out_dir: String::new(),
                capability_key: d.capability_id.clone(),
                mapping_kind: d.mapping_kind.clone(),
                status: "inventory_only".into(),
                compile_ok: Some(false),
                compile_log: Some(e.to_string()),
                files_written: 0,
                sdk_emit_template: None,
                notes: vec![format!("compile_adapter error: {e}")],
            }),
        }
    }
    let checked_n = adapters.iter().filter(|a| a.compile_ok.is_some()).count();
    let ok_n = adapters.iter().filter(|a| a.compile_ok == Some(true)).count();
    let compile_rate = if checked_n == 0 {
        0.0
    } else {
        ok_n as f64 / checked_n as f64
    };
    Ok(CompileBatchReport {
        package_root: package_root.display().to_string(),
        out_root: out_root.display().to_string(),
        adapters,
        compile_rate,
        notes: vec![
            format!("FORGE-008 batch: {checked_n} adapter(s), compile_rate={compile_rate:.2}"),
            "Does not write registry evidence (ADR-0007 / ADR-0010).".into(),
        ],
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdapterShape {
    BytesToJson,
    JsonToBytes,
    JsonToJson,
    Unknown,
}

fn classify_shape(draft: &AdapterDraft) -> AdapterShape {
    // Prefer capability id from ontology match.
    if draft.capability_id.starts_with("data.json.parse") {
        return AdapterShape::BytesToJson;
    }
    if draft.capability_id.starts_with("data.json.serialize") {
        return AdapterShape::JsonToBytes;
    }
    if draft.capability_id.starts_with("data.json.path_set") {
        return AdapterShape::JsonToJson;
    }
    // Fall back on stub signature heuristics.
    let sig = draft.signature.to_ascii_lowercase();
    if sig.contains("&[u8]") && (sig.contains("value") || sig.contains("result")) {
        return AdapterShape::BytesToJson;
    }
    if sig.contains("value") && (sig.contains("vec<u8>") || sig.contains("&[u8]")) {
        return AdapterShape::JsonToBytes;
    }
    AdapterShape::Unknown
}

fn upstream_call_path(package_name: &str, draft: &AdapterDraft) -> String {
    let pkg = package_name.replace('-', "_");
    // Prefer module path from AST extract notes / path.
    if let Some(mod_path) = module_from_source_path(&draft.source_path) {
        return format!("{pkg}::{mod_path}::{}", draft.candidate_name);
    }
    format!("{pkg}::{}", draft.candidate_name)
}

fn module_from_source_path(source_path: &str) -> Option<String> {
    // .../src/foo/bar.rs → foo::bar ; .../src/lib.rs → None
    let norm = source_path.replace('\\', "/");
    let idx = norm.rfind("/src/")?;
    let rel = &norm[idx + 5..];
    if rel == "lib.rs" || rel == "main.rs" || rel == "mod.rs" {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for p in rel.split('/') {
        if let Some(stem) = p.strip_suffix(".rs") {
            if stem != "mod" {
                parts.push(stem);
            }
        } else {
            parts.push(p);
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("::"))
    }
}

fn render_cargo_toml(
    crate_name: &str,
    package_name: &str,
    package_path: &str,
    shape: AdapterShape,
) -> String {
    let _ = shape;
    format!(
        r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"
publish = false
description = "Loom Forge FORGE-008 compileable adapter (not admitted)"

[dependencies]
serde_json = "1"
{package_name} = {{ path = "{package_path}" }}
"#
    )
}

fn render_adapter_lib(
    package_name: &str,
    draft: &AdapterDraft,
    call_path: &str,
    shape: AdapterShape,
) -> (String, Option<String>) {
    let impl_id = &draft.implementation_id;
    let cap_key = &draft.capability_id;
    let fn_name = &draft.candidate_name;
    let pkg_rust = package_name.replace('-', "_");

    match shape {
        AdapterShape::BytesToJson => {
            let template = format!("{pkg_rust}_forge_adapter::transform({{bytes}}.as_slice())?");
            // Use a stable export name `transform` for emit templates.
            let lib = format!(
                r#"//! FORGE-008 compileable adapter — NOT admitted.
//! Upstream: `{call_path}`
//! Implementation: {impl_id}
//! Capability: {cap_key}

use serde_json::Value;

pub const IMPLEMENTATION_ID: &str = "{impl_id}";
pub const CAPABILITY_KEY: &str = "{cap_key}";

/// Boundary entry: bytes → json.value
pub fn transform(bytes: &[u8]) -> Result<Value, String> {{
    {call_path}(bytes)
}}

/// Alias matching common pilot naming.
pub fn {fn_name}(bytes: &[u8]) -> Result<Value, String> {{
    transform(bytes)
}}
"#
            );
            // Fix template to use real crate name — filled by caller via crate_name.
            (lib, Some(template))
        }
        AdapterShape::JsonToBytes => {
            let template = format!("{pkg_rust}_forge_adapter::transform(&{{value}})?");
            let lib = format!(
                r#"//! FORGE-008 compileable adapter — NOT admitted.
//! Upstream: `{call_path}`
//! Implementation: {impl_id}
//! Capability: {cap_key}

use serde_json::Value;

pub const IMPLEMENTATION_ID: &str = "{impl_id}";
pub const CAPABILITY_KEY: &str = "{cap_key}";

/// Boundary entry: json.value → bytes
pub fn transform(value: &Value) -> Result<Vec<u8>, String> {{
    {call_path}(value)
}}

pub fn {fn_name}(value: &Value) -> Result<Vec<u8>, String> {{
    transform(value)
}}
"#
            );
            (lib, Some(template))
        }
        AdapterShape::JsonToJson => {
            // path_set needs path + value config — generate typed helper, not full config inline.
            let lib = format!(
                r#"//! FORGE-008 compileable adapter — NOT admitted.
//! Upstream: `{call_path}`
//! Implementation: {impl_id}
//! Capability: {cap_key}
//! Note: path_set requires config; call path_set(value, path, set_to).

use serde_json::Value;

pub const IMPLEMENTATION_ID: &str = "{impl_id}";
pub const CAPABILITY_KEY: &str = "{cap_key}";

pub fn path_set(value: Value, path: &str, set_to: Value) -> Result<Value, String> {{
    {call_path}(value, path, set_to)
}}
"#
            );
            (lib, None)
        }
        AdapterShape::Unknown => {
            let lib = format!(
                r#"//! FORGE-008 draft adapter (unknown shape) — NOT admitted.
//! Upstream: `{call_path}`
//! Original: {sig}

#![allow(unused)]

pub const IMPLEMENTATION_ID: &str = "{impl_id}";
pub const CAPABILITY_KEY: &str = "{cap_key}";

// Shape not recognized as bytes↔json pilot transform.
// Keep crate compiling with a placeholder that does not call upstream incorrectly.
pub fn not_wired() -> Result<(), String> {{
    Err("forge: unknown shape — map manually".into())
}}
"#,
                call_path = call_path,
                sig = draft.signature,
                impl_id = impl_id,
                cap_key = cap_key,
            );
            (lib, None)
        }
    }
}

fn sanitize_crate_name(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() || out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("wvx_{out}")
    } else {
        out
    }
}

fn write(path: &Path, contents: &str) -> Result<(), ForgeError> {
    fs::write(path, contents).map_err(|e| ForgeError::Io(path.to_path_buf(), e))
}

/// Cargo `path = "..."` on Windows rejects `\\?\` extended prefixes from canonicalize.
fn cargo_path_dep(path: &Path) -> String {
    let abs = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    let mut s = abs.display().to_string();
    for prefix in [r"\\?\UNC\", r"\\?\", r"//?/"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = if prefix.contains("UNC") {
                format!(r"\\{rest}")
            } else {
                rest.to_string()
            };
            break;
        }
    }
    s.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability_match::{OntologyCapability, OntologyPort};
    use crate::draft::draft_adapters_with_ontology;
    use std::path::PathBuf;

    #[test]
    fn module_path_from_source() {
        assert_eq!(
            module_from_source_path(r"C:\x\crates\wvx-adapters\src\serde_json_parse_owned.rs")
                .as_deref(),
            Some("serde_json_parse_owned")
        );
        assert_eq!(module_from_source_path("src/lib.rs"), None);
    }

    #[test]
    fn compile_external_demo_parse() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../wvx-adapter-external-demo");
        let ontology = vec![OntologyCapability {
            id: "data.json.parse".into(),
            version: "1".into(),
            kind: "transform".into(),
            inputs: vec![OntologyPort {
                id: "bytes".into(),
                ty: "bytes".into(),
            }],
            outputs: vec![OntologyPort {
                id: "value".into(),
                ty: "json_value".into(),
            }],
        }];
        let report = draft_adapters_with_ontology(&root, Some("upper_parse"), &ontology).unwrap();
        assert!(!report.drafts.is_empty());
        let d = &report.drafts[0];
        assert_eq!(d.mapping_kind, "exact_shape");
        let out = std::env::temp_dir().join(format!(
            "wvx-forge-compile-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let r = compile_adapter_from_draft(&root, &report.package_name, d, &out, true).unwrap();
        assert_eq!(r.compile_ok, Some(true), "log={:?}", r.compile_log);
        assert_eq!(r.status, "candidate");
        let _ = fs::remove_dir_all(&out);
    }
}
