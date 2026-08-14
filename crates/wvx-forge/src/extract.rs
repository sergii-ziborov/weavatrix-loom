//! Stage 2: static public API extraction and candidate shapes (no code execution).
//!
//! **ADR-0012:** this module is a *bootstrap* stand-in for Weavatrix code facts.
//! Prefer feeding signatures/spans from Weavatrix when available; do not grow
//! this into a full repository intelligence product.
//!
//! FORGE-004: prefer **syn AST** for multiline functions, impl methods, and types.
//! Falls back to line heuristics when a file fails to parse.

use crate::ForgeError;
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Fields, FnArg, Item, ItemEnum, ItemFn, ItemImpl, ItemStruct, ItemTrait, Pat, ReturnType, Type,
    Visibility,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    Function,
    Struct,
    Enum,
    Trait,
    Module,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCandidate {
    pub kind: CandidateKind,
    pub name: String,
    pub path: String,
    pub line: usize,
    /// Full-ish signature / declaration (from AST when possible).
    pub signature: String,
    /// Heuristic I/O shape for capability mapping.
    pub shape: CandidateShape,
    /// Extractor used: `ast` or `line`.
    #[serde(default = "default_extractor")]
    pub extractor: String,
    /// Rust module path relative to crate root when known (e.g. `serde_json_parse_owned`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_path: Option<String>,
    /// Weavatrix entity id when candidates come from facts (ADR-0012 source_ref).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_entity_id: Option<String>,
    /// Facts bundle / Weavatrix revision when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
}

fn default_extractor() -> String {
    "line".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CandidateShape {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractReport {
    pub root: String,
    pub package_name: String,
    pub candidates: Vec<ApiCandidate>,
    pub status: String,
    /// How many source files used AST vs line fallback.
    #[serde(default)]
    pub files_ast: usize,
    #[serde(default)]
    pub files_line_fallback: usize,
}

/// Extract public API surface from a single package directory (src/).
pub fn extract_public_api(package_root: impl AsRef<Path>) -> Result<ExtractReport, ForgeError> {
    let root = package_root.as_ref();
    if !root.is_dir() {
        return Err(ForgeError::NotDir(root.to_path_buf()));
    }
    let name = read_package_name(root).unwrap_or_else(|| {
        root.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    });

    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &mut files, 0)?;

    let mut candidates = Vec::new();
    let mut files_ast = 0usize;
    let mut files_line_fallback = 0usize;
    for file in files {
        let text = fs::read_to_string(&file).map_err(|e| ForgeError::Io(file.clone(), e))?;
        let file_mod = module_path_for_file(root, &file);
        match extract_from_ast(&file, &text, file_mod.as_deref()) {
            Ok(mut list) => {
                files_ast += 1;
                candidates.append(&mut list);
            }
            Err(note) => {
                files_line_fallback += 1;
                let mut list = Vec::new();
                extract_from_source_line(&file, &text, &mut list);
                for c in &mut list {
                    c.extractor = "line".into();
                    c.module_path = file_mod.clone();
                    c.shape.notes.push(format!("ast fallback: {note}"));
                }
                candidates.append(&mut list);
            }
        }
    }

    candidates.sort_by(|a, b| a.name.cmp(&b.name).then(a.line.cmp(&b.line)));

    let status = if candidates.is_empty() {
        "no_candidates".into()
    } else if files_line_fallback == 0 {
        "candidate_found_ast".into()
    } else {
        "candidate_found_mixed".into()
    };

    Ok(ExtractReport {
        root: root.display().to_string(),
        package_name: name,
        candidates,
        status,
        files_ast,
        files_line_fallback,
    })
}

fn module_path_for_file(package_root: &Path, file: &Path) -> Option<String> {
    let src = package_root.join("src");
    let rel = file.strip_prefix(&src).ok()?;
    let mut parts: Vec<String> = Vec::new();
    for comp in rel.components() {
        let s = comp.as_os_str().to_string_lossy();
        if s.ends_with(".rs") {
            let stem = s.trim_end_matches(".rs");
            if stem == "lib" || stem == "mod" || stem == "main" {
                continue;
            }
            parts.push(stem.to_string());
        } else {
            parts.push(s.to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("::"))
    }
}

fn extract_from_ast(
    path: &Path,
    text: &str,
    file_module: Option<&str>,
) -> Result<Vec<ApiCandidate>, String> {
    let file = syn::parse_file(text).map_err(|e| e.to_string())?;
    let mut v = AstCollector {
        path: path.display().to_string(),
        file_module: file_module.map(str::to_string),
        module_stack: Vec::new(),
        out: Vec::new(),
    };
    v.visit_file(&file);
    Ok(v.out)
}

struct AstCollector {
    path: String,
    file_module: Option<String>,
    module_stack: Vec<String>,
    out: Vec<ApiCandidate>,
}

impl AstCollector {
    fn current_module(&self) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(ref fm) = self.file_module {
            parts.push(fm.clone());
        }
        parts.extend(self.module_stack.iter().cloned());
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("::"))
        }
    }

    fn line_of<T: Spanned>(&self, node: &T) -> usize {
        node.span().start().line
    }

    fn push_fn(&mut self, item: &ItemFn, inherent_self_ty: Option<&str>) {
        if !is_public(&item.vis) {
            return;
        }
        let name = item.sig.ident.to_string();
        let signature = item.sig.to_token_stream().to_string().replace('\n', " ");
        let mut shape = shape_from_syn_sig(&item.sig);
        if let Some(ty) = inherent_self_ty {
            shape.notes.push(format!("impl method on {ty}"));
        }
        if item.sig.asyncness.is_some() {
            shape.notes.push("async".into());
        }
        self.out.push(ApiCandidate {
            kind: CandidateKind::Function,
            name,
            path: self.path.clone(),
            line: self.line_of(item),
            signature: signature.chars().take(300).collect(),
            shape,
            extractor: "ast".into(),
            module_path: self.current_module(),
            source_entity_id: None,
            source_revision: None,
        });
    }
}

impl<'ast> Visit<'ast> for AstCollector {
    fn visit_item(&mut self, item: &'ast Item) {
        match item {
            Item::Fn(f) => self.push_fn(f, None),
            Item::Struct(s) => self.push_struct(s),
            Item::Enum(e) => self.push_enum(e),
            Item::Trait(t) => self.push_trait(t),
            Item::Impl(i) => self.visit_item_impl(i),
            Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    let name = m.ident.to_string();
                    self.module_stack.push(name);
                    for it in items {
                        self.visit_item(it);
                    }
                    self.module_stack.pop();
                }
            }
            _ => {}
        }
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        // Skip trait impls for adapter candidates (inherent methods only).
        if i.trait_.is_some() {
            return;
        }
        let self_ty = type_to_string(&i.self_ty);
        for item in &i.items {
            if let syn::ImplItem::Fn(method) = item {
                if !is_public(&method.vis) {
                    continue;
                }
                // Reuse ItemFn-shaped push via temporary conversion of signature.
                let name = method.sig.ident.to_string();
                let signature = method.sig.to_token_stream().to_string().replace('\n', " ");
                let mut shape = shape_from_syn_sig(&method.sig);
                shape.notes.push(format!("impl method on {self_ty}"));
                self.out.push(ApiCandidate {
                    kind: CandidateKind::Function,
                    name,
                    path: self.path.clone(),
                    line: self.line_of(method),
                    signature: signature.chars().take(300).collect(),
                    shape,
                    extractor: "ast".into(),
                    module_path: self.current_module(),
                    source_entity_id: None,
                    source_revision: None,
                });
            }
        }
    }
}

impl AstCollector {
    fn push_struct(&mut self, s: &ItemStruct) {
        if !is_public(&s.vis) {
            return;
        }
        let name = s.ident.to_string();
        let fields = match &s.fields {
            Fields::Named(n) => format!("{{ {} fields }}", n.named.len()),
            Fields::Unnamed(u) => format!("( {} fields )", u.unnamed.len()),
            Fields::Unit => "unit".into(),
        };
        self.out.push(ApiCandidate {
            kind: CandidateKind::Struct,
            name,
            path: self.path.clone(),
            line: self.line_of(s),
            signature: format!("pub struct {} {fields}", s.ident),
            shape: CandidateShape {
                notes: vec!["type definition".into()],
                ..Default::default()
            },
            extractor: "ast".into(),
            module_path: self.current_module(),
            source_entity_id: None,
            source_revision: None,
        });
    }

    fn push_enum(&mut self, e: &ItemEnum) {
        if !is_public(&e.vis) {
            return;
        }
        self.out.push(ApiCandidate {
            kind: CandidateKind::Enum,
            name: e.ident.to_string(),
            path: self.path.clone(),
            line: self.line_of(e),
            signature: format!("pub enum {} {{ {} variants }}", e.ident, e.variants.len()),
            shape: CandidateShape {
                notes: vec!["type definition".into()],
                ..Default::default()
            },
            extractor: "ast".into(),
            module_path: self.current_module(),
            source_entity_id: None,
            source_revision: None,
        });
    }

    fn push_trait(&mut self, t: &ItemTrait) {
        if !is_public(&t.vis) {
            return;
        }
        self.out.push(ApiCandidate {
            kind: CandidateKind::Trait,
            name: t.ident.to_string(),
            path: self.path.clone(),
            line: self.line_of(t),
            signature: format!("pub trait {}", t.ident),
            shape: CandidateShape {
                notes: vec!["trait definition".into()],
                ..Default::default()
            },
            extractor: "ast".into(),
            module_path: self.current_module(),
            source_entity_id: None,
            source_revision: None,
        });
    }
}

fn is_public(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn shape_from_syn_sig(sig: &syn::Signature) -> CandidateShape {
    let mut shape = CandidateShape::default();
    for arg in &sig.inputs {
        match arg {
            FnArg::Receiver(_) => {
                shape.notes.push("has receiver".into());
            }
            FnArg::Typed(pat_ty) => {
                if is_self_pat(&pat_ty.pat) {
                    continue;
                }
                shape
                    .inputs
                    .push(normalize_rust_type(&type_to_string(&pat_ty.ty)));
            }
        }
    }
    match &sig.output {
        ReturnType::Default => shape.outputs.push("unit".into()),
        ReturnType::Type(_, ty) => {
            shape.outputs.push(normalize_rust_type(&type_to_string(ty)));
        }
    }
    if shape.inputs.iter().any(|t| t == "bytes" || t == "string")
        && shape
            .outputs
            .iter()
            .any(|t| t == "json.value" || t == "bytes" || t == "string")
    {
        shape.notes.push("possible data transform".into());
    }
    shape
}

fn is_self_pat(pat: &Pat) -> bool {
    matches!(pat, Pat::Ident(i) if i.ident == "self")
}

fn type_to_string(ty: &Type) -> String {
    ty.to_token_stream().to_string().replace(' ', "")
}

fn read_package_name(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("Cargo.toml")).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("name") {
            let rest = rest.trim().trim_start_matches('=').trim();
            let name = rest.trim_matches('"').trim_matches('\'');
            if !name.is_empty() && !name.contains('{') {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) -> Result<(), ForgeError> {
    if depth > 8 || !dir.is_dir() {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|e| ForgeError::Io(dir.to_path_buf(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| ForgeError::Io(dir.to_path_buf(), e))?;
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "target" || name.starts_with('.') {
                continue;
            }
            collect_rs_files(&path, out, depth + 1)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    Ok(())
}

// --- line fallback (pre-AST / unparseable files) --------------------------------

fn extract_from_source_line(path: &Path, text: &str, out: &mut Vec<ApiCandidate>) {
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("///") {
            continue;
        }
        if let Some(name) = match_pub_fn(trimmed) {
            let shape = shape_from_fn_sig(trimmed);
            out.push(ApiCandidate {
                kind: CandidateKind::Function,
                name,
                path: path.display().to_string(),
                line: idx + 1,
                signature: trimmed.chars().take(200).collect(),
                shape,
                extractor: "line".into(),
                module_path: None,
                source_entity_id: None,
                source_revision: None,
            });
        } else if let Some(name) = match_pub_item(trimmed, "struct") {
            out.push(ApiCandidate {
                kind: CandidateKind::Struct,
                name,
                path: path.display().to_string(),
                line: idx + 1,
                signature: trimmed.chars().take(200).collect(),
                shape: CandidateShape {
                    notes: vec!["type definition".into()],
                    ..Default::default()
                },
                extractor: "line".into(),
                module_path: None,
                source_entity_id: None,
                source_revision: None,
            });
        } else if let Some(name) = match_pub_item(trimmed, "enum") {
            out.push(ApiCandidate {
                kind: CandidateKind::Enum,
                name,
                path: path.display().to_string(),
                line: idx + 1,
                signature: trimmed.chars().take(200).collect(),
                shape: CandidateShape {
                    notes: vec!["type definition".into()],
                    ..Default::default()
                },
                extractor: "line".into(),
                module_path: None,
                source_entity_id: None,
                source_revision: None,
            });
        } else if let Some(name) = match_pub_item(trimmed, "trait") {
            out.push(ApiCandidate {
                kind: CandidateKind::Trait,
                name,
                path: path.display().to_string(),
                line: idx + 1,
                signature: trimmed.chars().take(200).collect(),
                shape: CandidateShape {
                    notes: vec!["trait definition".into()],
                    ..Default::default()
                },
                extractor: "line".into(),
                module_path: None,
                source_entity_id: None,
                source_revision: None,
            });
        }
    }
}

fn match_pub_fn(line: &str) -> Option<String> {
    let line = line.trim_start();
    if !line.starts_with("pub fn ")
        && !line.starts_with("pub async fn ")
        && !line.starts_with("pub const fn ")
    {
        return None;
    }
    let after = line.find("fn ").map(|i| &line[i + 3..])?.trim_start();
    let name: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn match_pub_item(line: &str, kind: &str) -> Option<String> {
    let prefix = format!("pub {kind} ");
    let line = line.trim_start();
    if !line.starts_with(&prefix) {
        return None;
    }
    let after = line[prefix.len()..].trim_start();
    let name: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn shape_from_fn_sig(sig: &str) -> CandidateShape {
    let mut shape = CandidateShape::default();
    if let Some(ret) = sig.split("->").nth(1) {
        let ret = ret.trim().trim_end_matches('{').trim();
        if !ret.is_empty() {
            shape.outputs.push(normalize_rust_type(ret));
        }
    } else {
        shape.outputs.push("unit".into());
    }
    if let Some(start) = sig.find('(') {
        if let Some(end) = sig[start + 1..].find(')') {
            let params = &sig[start + 1..start + 1 + end];
            for part in params.split(',') {
                let part = part.trim();
                if part.is_empty() || part == "&self" || part == "&mut self" || part == "self" {
                    continue;
                }
                if let Some((_, ty)) = part.rsplit_once(':') {
                    shape.inputs.push(normalize_rust_type(ty.trim()));
                } else {
                    shape.inputs.push(part.to_string());
                }
            }
        }
    }
    shape
}

pub(crate) fn normalize_rust_type(ty: &str) -> String {
    let full = ty.trim().trim_start_matches("pub ").trim();
    let compact = full.replace(' ', "");
    let lower = compact.to_ascii_lowercase();
    if lower.contains("&[u8]") || lower.contains("vec<u8>") {
        return "bytes".into();
    }
    if lower.contains("serde_json::value")
        || lower.contains("json::value")
        || (lower.contains("value")
            && (lower.contains("result") || lower.ends_with("value") || lower.contains("value,")))
    {
        if lower.contains("value") {
            return "json.value".into();
        }
    }
    if compact == "Value" || compact.ends_with("::Value") {
        return "json.value".into();
    }
    if lower == "&str" || lower == "string" || lower == "&string" {
        return "string".into();
    }

    let t = compact.split('<').next().unwrap_or(&compact).trim();
    match t {
        "&[u8]" | "Vec<u8>" | "&Vec<u8>" => "bytes".into(),
        "&str" | "String" | "&String" => "string".into(),
        "bool" => "bool".into(),
        "i64" | "i32" | "isize" => "i64".into(),
        "u64" | "u32" | "usize" => "u64".into(),
        "f64" | "f32" => "f64".into(),
        "()" => "unit".into(),
        "Value" => "json.value".into(),
        other if other.contains("Value") => "json.value".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_from_wvx_types_ast() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../wvx-types");
        let report = extract_public_api(&root).unwrap();
        assert!(report.files_ast > 0 || report.files_line_fallback > 0);
        assert!(
            report.candidates.iter().any(|c| {
                c.name.contains("Type")
                    || c.kind == CandidateKind::Enum
                    || matches!(c.kind, CandidateKind::Struct)
            }),
            "expected type definitions"
        );
    }

    #[test]
    fn parse_fn_shape() {
        let shape = shape_from_fn_sig("pub fn parse(bytes: &[u8]) -> Result<Value, String> {");
        assert!(shape.inputs.iter().any(|i| i == "bytes"));
        assert!(shape.outputs.iter().any(|o| o == "json.value"));
    }

    #[test]
    fn serialize_fn_shape() {
        let shape = shape_from_fn_sig("pub fn to_vec(v: &Value) -> Result<Vec<u8>, String> {");
        assert!(shape.inputs.iter().any(|i| i == "json.value"));
        assert!(shape.outputs.iter().any(|o| o == "bytes"));
    }

    #[test]
    fn ast_multiline_function() {
        let src = r#"
            pub fn multi(
                bytes: &[u8],
                _hint: bool,
            ) -> Result<serde_json::Value, String> {
                todo!()
            }
        "#;
        let list = extract_from_ast(Path::new("src/lib.rs"), src, None).unwrap();
        let f = list.iter().find(|c| c.name == "multi").expect("multi");
        assert_eq!(f.extractor, "ast");
        assert!(f.shape.inputs.iter().any(|i| i == "bytes"));
        assert!(f.shape.outputs.iter().any(|o| o == "json.value"));
    }

    #[test]
    fn extracts_adapters_with_module_path() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../wvx-adapters");
        let report = extract_public_api(&root).unwrap();
        let parse = report
            .candidates
            .iter()
            .find(|c| {
                c.name == "parse" && c.module_path.as_deref() == Some("serde_json_parse_owned")
            })
            .expect("serde_json_parse_owned::parse");
        assert_eq!(parse.extractor, "ast");
        assert!(parse.shape.inputs.iter().any(|i| i == "bytes"));
        assert!(parse.shape.outputs.iter().any(|o| o == "json.value"));
    }
}
