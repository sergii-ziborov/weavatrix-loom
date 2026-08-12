//! Stage 3: static **adapter draft** from extract candidates.
//!
//! Produces capability + implementation manifest sketches and a Rust stub.
//! Never executes code, never writes admission — status is always `inventory_only`.

use crate::extract::{extract_public_api, ApiCandidate, CandidateKind, ExtractReport};
use crate::ForgeError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterDraft {
    pub candidate_name: String,
    pub candidate_kind: String,
    pub source_path: String,
    pub source_line: usize,
    pub signature: String,
    /// Always `inventory_only` for Forge drafts (ADR-0007).
    pub status: String,
    pub capability_id: String,
    pub implementation_id: String,
    /// Pretty JSON capability contract draft.
    pub capability_json: String,
    /// Pretty JSON implementation manifest draft.
    pub implementation_json: String,
    /// Suggested Rust adapter stub (not compiled).
    pub adapter_stub_rs: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftReport {
    pub root: String,
    pub package_name: String,
    pub package_version: String,
    pub drafts: Vec<AdapterDraft>,
    /// `draft_ready` when ≥1 draft; `no_function_candidates` otherwise.
    pub status: String,
    pub notes: Vec<String>,
}

/// Draft adapters for all public **functions** under a package (optional name filter).
pub fn draft_adapters(
    package_root: impl AsRef<Path>,
    name_filter: Option<&str>,
) -> Result<DraftReport, ForgeError> {
    let root = package_root.as_ref();
    let extract = extract_public_api(root)?;
    draft_from_extract(&extract, name_filter, root)
}

/// Build drafts from an existing extract report.
pub fn draft_from_extract(
    extract: &ExtractReport,
    name_filter: Option<&str>,
    package_root: &Path,
) -> Result<DraftReport, ForgeError> {
    let version = read_package_version(package_root).unwrap_or_else(|| "0.0.0".into());
    let license = read_package_license(package_root);
    let filter = name_filter.map(|s| s.to_ascii_lowercase());

    let mut drafts = Vec::new();
    for c in &extract.candidates {
        if c.kind != CandidateKind::Function {
            continue;
        }
        if let Some(ref f) = filter {
            if !c.name.to_ascii_lowercase().contains(f) {
                continue;
            }
        }
        drafts.push(draft_one(
            &extract.package_name,
            &version,
            license.as_deref(),
            c,
        ));
    }

    drafts.sort_by(|a, b| a.implementation_id.cmp(&b.implementation_id));

    let mut notes = vec![
        "Static draft only — not admitted, not wired into playground/export.".into(),
        "Review shapes, then promote via registry + conformance before use.".into(),
    ];
    if drafts.is_empty() {
        notes.push("No public `fn` candidates matched (structs/traits are skipped).".into());
    }

    Ok(DraftReport {
        root: extract.root.clone(),
        package_name: extract.package_name.clone(),
        package_version: version,
        status: if drafts.is_empty() {
            "no_function_candidates".into()
        } else {
            "draft_ready".into()
        },
        drafts,
        notes,
    })
}

/// Optionally write draft files under `out_dir/{impl_id}/`.
pub fn write_draft_files(report: &DraftReport, out_dir: impl AsRef<Path>) -> Result<usize, ForgeError> {
    let out = out_dir.as_ref();
    fs::create_dir_all(out).map_err(|e| ForgeError::Io(out.to_path_buf(), e))?;
    let mut n = 0;
    for d in &report.drafts {
        let dir = out.join(sanitize_file_component(&d.implementation_id));
        fs::create_dir_all(&dir).map_err(|e| ForgeError::Io(dir.clone(), e))?;
        write_file(&dir.join("capability.json"), &d.capability_json)?;
        write_file(&dir.join("implementation.json"), &d.implementation_json)?;
        write_file(&dir.join("adapter_stub.rs"), &d.adapter_stub_rs)?;
        write_file(
            &dir.join("README.md"),
            &format!(
                "# Forge draft: {}\n\nStatus: **{}** (not admitted).\n\nSource: `{}` L{}\n\n```\n{}\n```\n\nNotes:\n{}\n",
                d.implementation_id,
                d.status,
                d.source_path,
                d.source_line,
                d.signature,
                d.notes.iter().map(|n| format!("- {n}")).collect::<Vec<_>>().join("\n")
            ),
        )?;
        n += 1;
    }
    Ok(n)
}

fn write_file(path: &Path, contents: &str) -> Result<(), ForgeError> {
    fs::write(path, contents).map_err(|e| ForgeError::Io(path.to_path_buf(), e))
}

fn draft_one(
    package_name: &str,
    package_version: &str,
    license: Option<&str>,
    c: &ApiCandidate,
) -> AdapterDraft {
    let pkg_slug = slugify(package_name);
    let fn_slug = slugify(&c.name);
    let mod_slug = path_module_slug(&c.path);
    let cap_id = format!("{pkg_slug}.{fn_slug}");
    // Disambiguate same fn name in different modules.
    let impl_id = if mod_slug.is_empty() || mod_slug == "lib" || mod_slug == "mod" {
        format!("forge.{pkg_slug}.{fn_slug}@1")
    } else {
        format!("forge.{pkg_slug}.{mod_slug}.{fn_slug}@1")
    };

    let inputs = ports_from_types(&c.shape.inputs, "in");
    let outputs = ports_from_types(&c.shape.outputs, "out");

    let mut notes = c.shape.notes.clone();
    notes.push("Generated by Loom Forge draft (static).".into());
    notes.push(format!("Upstream signature: {}", c.signature));
    if outputs.iter().any(|p| p.ty.contains("Result") || p.ty == "unit") {
        notes.push("Return type may need manual Result unwrap / mapping.".into());
    }

    let capability = serde_json::json!({
        "id": cap_id,
        "version": "1",
        "kind": if inputs.is_empty() && outputs.is_empty() { "unknown" } else { "transform" },
        "inputs": inputs.iter().map(|p| serde_json::json!({
            "id": p.id,
            "type": p.ty,
            "required": true
        })).collect::<Vec<_>>(),
        "outputs": outputs.iter().map(|p| serde_json::json!({
            "id": p.id,
            "type": p.ty,
            "required": true
        })).collect::<Vec<_>>(),
        "errors": ["invalid-input", "upstream-error"],
        "effects": []
    });

    let license_fact = if license.is_some() { "pass" } else { "absent" };
    let impl_base_id = impl_id.trim_end_matches("@1");
    let implementation = serde_json::json!({
        "id": impl_base_id,
        "version": "1",
        "capability": { "id": cap_id, "version": "1" },
        "source": {
            "kind": "forge-draft",
            "package": package_name,
            "package_version": package_version,
            "notes": format!("{}:{}", c.path, c.line)
        },
        "adapter": {
            "crate_name": format!("wvx-adapter-{}", slugify(impl_base_id)),
            "execution": "native-rust"
        },
        "status": "inventory_only",
        "evidence": {
            "build": "absent",
            "conformance": "absent",
            "benchmark": "absent",
            "license": license_fact,
            "security": "absent"
        },
        "notes": "Forge static draft — not admitted. Run conformance before promoting."
    });

    let adapter_stub_rs = render_stub(package_name, &c.name, &inputs, &outputs, &c.signature);

    AdapterDraft {
        candidate_name: c.name.clone(),
        candidate_kind: match c.kind {
            CandidateKind::Function => "function".into(),
            CandidateKind::Struct => "struct".into(),
            CandidateKind::Enum => "enum".into(),
            CandidateKind::Trait => "trait".into(),
            CandidateKind::Module => "module".into(),
        },
        source_path: c.path.clone(),
        source_line: c.line,
        signature: c.signature.clone(),
        status: "inventory_only".into(),
        capability_id: format!("{cap_id}@1"),
        implementation_id: impl_id,
        capability_json: pretty(&capability),
        implementation_json: pretty(&implementation),
        adapter_stub_rs,
        notes,
    }
}

struct PortDraft {
    id: String,
    ty: String,
}

fn ports_from_types(types: &[String], prefix: &str) -> Vec<PortDraft> {
    if types.is_empty() {
        return Vec::new();
    }
    if types.len() == 1 {
        let ty = map_boundary_type(&types[0]);
        let id = default_port_id(&ty, prefix);
        return vec![PortDraft { id, ty }];
    }
    types
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let ty = map_boundary_type(t);
            PortDraft {
                id: format!("{prefix}{}", i),
                ty,
            }
        })
        .collect()
}

fn default_port_id(ty: &str, prefix: &str) -> String {
    match ty {
        "bytes" => {
            if prefix == "out" {
                "bytes".into()
            } else {
                "bytes".into()
            }
        }
        "json.value" | "json_value" => "value".into(),
        "string" => "text".into(),
        "bool" => "flag".into(),
        "unit" => "unit".into(),
        _ => {
            if prefix == "out" {
                "value".into()
            } else {
                "input".into()
            }
        }
    }
}

fn map_boundary_type(ty: &str) -> String {
    let t = ty.trim();
    if t.contains("Result") {
        // keep as opaque note type for draft
        if t.contains("Value") {
            return "json.value".into();
        }
        if t.contains("String") {
            return "string".into();
        }
        if t.contains("Vec<u8>") || t.contains("[u8]") {
            return "bytes".into();
        }
        return "json.value".into();
    }
    match t {
        "bytes" | "string" | "bool" | "i64" | "u64" | "f64" | "unit" | "json.value" => t.into(),
        other => other.to_string(),
    }
}

fn render_stub(
    package: &str,
    fn_name: &str,
    inputs: &[PortDraft],
    outputs: &[PortDraft],
    signature: &str,
) -> String {
    let mut args = Vec::new();
    for p in inputs {
        let rust_ty = rust_ty_for_boundary(&p.ty);
        args.push(format!("{}: {}", p.id, rust_ty));
    }
    let ret = if outputs.is_empty() {
        "()".into()
    } else if outputs.len() == 1 {
        rust_ty_for_boundary(&outputs[0].ty)
    } else {
        format!(
            "({})",
            outputs
                .iter()
                .map(|p| rust_ty_for_boundary(&p.ty))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let arg_list = args.join(", ");
    format!(
        r#"//! DRAFT adapter stub — Loom Forge (static). NOT admitted.
//! Upstream: `{package}::{fn_name}`
//! Original: {signature}
//!
//! Wire this to the real crate, then run conformance before promoting
//! status past `inventory_only`.

#![allow(unused)]

pub fn {fn_name}({arg_list}) -> Result<{ret}, String> {{
    // TODO: call `{package}::{fn_name}` and map boundary types.
    Err("forge draft: not implemented".into())
}}
"#,
        package = package,
        fn_name = fn_name,
        signature = signature,
        arg_list = arg_list,
        ret = ret,
    )
}

fn rust_ty_for_boundary(ty: &str) -> String {
    match ty {
        "bytes" => "Vec<u8>".into(),
        "string" => "String".into(),
        "bool" => "bool".into(),
        "i64" => "i64".into(),
        "u64" => "u64".into(),
        "f64" => "f64".into(),
        "unit" => "()".into(),
        "json.value" | "json_value" => "serde_json::Value".into(),
        other => other.to_string(),
    }
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c == '_' || c == '-' || c == '.' {
            if !out.ends_with('_') {
                out.push('_');
            }
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        "item".into()
    } else {
        out
    }
}

fn sanitize_file_component(s: &str) -> String {
    s.replace(['/', '\\', ':', '@'], "_")
}

fn pretty(v: &serde_json::Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

fn path_module_slug(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let name = normalized
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".rs");
    slugify(name)
}

fn read_package_version(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("Cargo.toml")).ok()?;
    let mut in_package = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if in_package && t.starts_with("version") {
            let rest = t.trim_start_matches("version").trim().trim_start_matches('=').trim();
            if rest.contains("workspace") {
                return Some("workspace".into());
            }
            let v = rest.trim_matches('"').trim_matches('\'');
            if !v.is_empty() {
                return Some(v.into());
            }
        }
    }
    None
}

fn read_package_license(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("Cargo.toml")).ok()?;
    let mut in_package = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if in_package && t.starts_with("license") {
            let rest = t.trim_start_matches("license").trim().trim_start_matches('=').trim();
            let v = rest.trim_matches('"').trim_matches('\'');
            if !v.is_empty() && !v.starts_with('{') {
                return Some(v.into());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn drafts_functions_from_wvx_types() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../wvx-types");
        let report = draft_adapters(&root, None).unwrap();
        // wvx-types is mostly types; may have few pub fn — still ok if empty or non-empty
        assert!(
            report.status == "draft_ready" || report.status == "no_function_candidates",
            "{}",
            report.status
        );
    }

    #[test]
    fn draft_known_parse_shape() {
        use crate::extract::{ApiCandidate, CandidateKind, CandidateShape};
        let c = ApiCandidate {
            kind: CandidateKind::Function,
            name: "parse".into(),
            path: "src/lib.rs".into(),
            line: 10,
            signature: "pub fn parse(bytes: &[u8]) -> Result<Value, String> {".into(),
            shape: CandidateShape {
                inputs: vec!["bytes".into()],
                outputs: vec!["json.value".into()],
                notes: vec!["possible data transform".into()],
            },
        };
        let d = draft_one("demo-json", "0.1.0", Some("MIT"), &c);
        assert_eq!(d.status, "inventory_only");
        assert!(d.capability_json.contains("demo_json.parse") || d.capability_id.contains("parse"));
        assert!(d.implementation_json.contains("inventory_only"));
        assert!(d.adapter_stub_rs.contains("pub fn parse"));
        assert!(d.adapter_stub_rs.contains("NOT admitted"));
    }
}
