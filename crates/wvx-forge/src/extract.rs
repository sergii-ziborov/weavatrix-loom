//! Stage 2: static public API extraction and candidate shapes (no code execution).

use crate::ForgeError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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
    /// Rough signature / declaration line (trimmed).
    pub signature: String,
    /// Heuristic I/O shape for capability mapping (v0.1).
    pub shape: CandidateShape,
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
    for file in files {
        let text = fs::read_to_string(&file).map_err(|e| ForgeError::Io(file.clone(), e))?;
        extract_from_source(&file, &text, &mut candidates);
    }

    candidates.sort_by(|a, b| a.name.cmp(&b.name).then(a.line.cmp(&b.line)));

    Ok(ExtractReport {
        root: root.display().to_string(),
        package_name: name,
        candidates,
        status: "candidate_found".into(),
    })
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

fn extract_from_source(path: &Path, text: &str, out: &mut Vec<ApiCandidate>) {
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
            });
        }
    }
}

fn match_pub_fn(line: &str) -> Option<String> {
    // pub fn name / pub async fn name / pub(crate) fn — skip crate-private
    let line = line.trim_start();
    if !line.starts_with("pub fn ")
        && !line.starts_with("pub async fn ")
        && !line.starts_with("pub const fn ")
    {
        return None;
    }
    let after = line
        .find("fn ")
        .map(|i| &line[i + 3..])?
        .trim_start();
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
    // Very rough: look at return type after `->`
    if let Some(ret) = sig.split("->").nth(1) {
        let ret = ret.trim().trim_end_matches('{').trim();
        if !ret.is_empty() {
            shape.outputs.push(normalize_rust_type(ret));
        }
    } else {
        shape.outputs.push("unit".into());
    }
    // Parameters between first ( and matching ) — simplified first paren pair
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
    if shape.inputs.iter().any(|t| t.contains("[u8]") || t.contains("Vec<u8>") || t.contains("&str") || t.contains("String"))
        && shape.outputs.iter().any(|t| t.contains("Value") || t.contains("String") || t.contains("Vec"))
    {
        shape.notes.push("possible data transform".into());
    }
    shape
}

fn normalize_rust_type(ty: &str) -> String {
    let t = ty
        .trim()
        .trim_start_matches("pub ")
        .split('<')
        .next()
        .unwrap_or(ty)
        .trim();
    match t {
        "&[u8]" | "Vec<u8>" | "&Vec<u8>" => "bytes".into(),
        "&str" | "String" | "&String" => "string".into(),
        "bool" => "bool".into(),
        "i64" | "i32" | "isize" => "i64".into(),
        "u64" | "u32" | "usize" => "u64".into(),
        "f64" | "f32" => "f64".into(),
        "()" => "unit".into(),
        other if other.contains("Value") => "json.value".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_from_wvx_types() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../wvx-types");
        let report = extract_public_api(&root).unwrap();
        assert!(
            report.candidates.iter().any(|c| c.name.contains("Type") || c.kind == CandidateKind::Enum || matches!(c.kind, CandidateKind::Struct)),
            "expected type definitions, got {:?}",
            report.candidates.iter().map(|c| &c.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn parse_fn_shape() {
        let shape = shape_from_fn_sig("pub fn parse(bytes: &[u8]) -> Result<Value, String> {");
        assert!(shape.inputs.iter().any(|i| i == "bytes"));
        assert!(shape.outputs.iter().any(|o| o == "json.value" || o.contains("Result")));
    }
}
