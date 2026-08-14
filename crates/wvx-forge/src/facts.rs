//! Weavatrix **code facts** wire format for Forge (ADR-0012).
//!
//! Target product path:
//! ```text
//! Weavatrix (UNDERSTAND) → facts JSON → Forge match/draft → Registry
//! ```
//!
//! Bootstrap Cargo/AST extract remains available offline, but when a facts
//! bundle is supplied it is the preferred candidate source. Loom does **not**
//! embed Weavatrix; it only accepts this interchange document (file/HTTP).

use crate::extract::{ApiCandidate, CandidateKind, CandidateShape, ExtractReport};
use crate::ForgeError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Schema id for the facts interchange document.
pub const FACTS_SCHEMA_VERSION: &str = "wvx.facts.v0.1";

/// Provider of Weavatrix code facts for Forge (ADR-0012).
///
/// Prefer this over growing product-level AST/index inside Loom. Bootstrap AST
/// extract implements the same trait only as a **fallback**.
pub trait WeavatrixFactsProvider {
    fn load_facts(&self) -> Result<WeavatrixFactsBundle, ForgeError>;
}

/// Load facts from a JSON file on disk.
pub struct FileFactsProvider {
    pub path: std::path::PathBuf,
}

impl WeavatrixFactsProvider for FileFactsProvider {
    fn load_facts(&self) -> Result<WeavatrixFactsBundle, ForgeError> {
        load_facts_file(&self.path)
    }
}

/// In-memory / HTTP body facts.
pub struct JsonFactsProvider {
    pub json: String,
}

impl WeavatrixFactsProvider for JsonFactsProvider {
    fn load_facts(&self) -> Result<WeavatrixFactsBundle, ForgeError> {
        parse_facts_json(&self.json)
            .map_err(|e| ForgeError::Parse(std::path::PathBuf::from("<json>"), e))
    }
}

/// **Bootstrap only:** run local AST extract and export as facts.
///
/// Not the product path — deep code intelligence belongs to Weavatrix.
pub struct BootstrapAstFactsProvider {
    pub package_root: std::path::PathBuf,
}

impl WeavatrixFactsProvider for BootstrapAstFactsProvider {
    fn load_facts(&self) -> Result<WeavatrixFactsBundle, ForgeError> {
        let extract = crate::extract::extract_public_api(&self.package_root)?;
        Ok(facts_from_extract(&extract, "bootstrap-ast"))
    }
}

/// Bundle of code facts produced by Weavatrix (or exported from bootstrap extract).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeavatrixFactsBundle {
    /// Must be [`FACTS_SCHEMA_VERSION`] for v0.1.
    pub schema_version: String,
    /// Provenance: `weavatrix` | `bootstrap-export` | `manual` | other.
    #[serde(default = "default_source")]
    pub source: String,
    /// Package / crate identity these facts describe.
    pub package_name: String,
    /// Optional filesystem root (for drafts that need a path label).
    #[serde(default)]
    pub package_root: Option<String>,
    /// Optional package version string.
    #[serde(default)]
    pub package_version: Option<String>,
    /// Weavatrix index / export revision (optional; flows into `source_ref.revision`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Fact entities (functions, types, …).
    #[serde(default)]
    pub entities: Vec<WeavatrixFactEntity>,
    #[serde(default)]
    pub notes: Vec<String>,
}

fn default_source() -> String {
    "weavatrix".into()
}

/// One code entity as understood by Weavatrix (or bootstrap export).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeavatrixFactEntity {
    /// `function` | `struct` | `enum` | `trait` | `module` (case-insensitive).
    pub kind: String,
    pub name: String,
    /// Stable Weavatrix entity id when available (preferred over path#line).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    /// Source path relative or absolute.
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub line: usize,
    /// Full-ish signature / declaration text.
    #[serde(default)]
    pub signature: String,
    /// Rust module path when known (e.g. `serde_json_parse_owned`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_path: Option<String>,
    /// Input type labels for capability matching (e.g. `bytes`, `&[u8]`).
    #[serde(default)]
    pub inputs: Vec<String>,
    /// Output type labels (e.g. `json_value`, `Result<Value, E>`).
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Load a facts bundle from a JSON file.
pub fn load_facts_file(path: impl AsRef<Path>) -> Result<WeavatrixFactsBundle, ForgeError> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|e| ForgeError::Io(path.to_path_buf(), e))?;
    parse_facts_json(&text).map_err(|e| ForgeError::Parse(path.to_path_buf(), e))
}

/// Parse facts JSON from a string.
pub fn parse_facts_json(text: &str) -> Result<WeavatrixFactsBundle, String> {
    let bundle: WeavatrixFactsBundle =
        serde_json::from_str(text).map_err(|e| format!("facts JSON: {e}"))?;
    validate_facts(&bundle)?;
    Ok(bundle)
}

/// Light validation — fail closed on empty / wrong schema when strict.
pub fn validate_facts(bundle: &WeavatrixFactsBundle) -> Result<(), String> {
    if bundle.schema_version.trim().is_empty() {
        return Err("facts.schema_version is required".into());
    }
    if bundle.schema_version != FACTS_SCHEMA_VERSION {
        // Accept future-compatible prefixes only if documented; for now require exact.
        return Err(format!(
            "unsupported facts schema_version `{}` (expected `{FACTS_SCHEMA_VERSION}`)",
            bundle.schema_version
        ));
    }
    if bundle.package_name.trim().is_empty() {
        return Err("facts.package_name is required".into());
    }
    Ok(())
}

/// Convert Weavatrix facts → Forge [`ExtractReport`] (candidate shapes for match/draft).
pub fn extract_from_facts(bundle: &WeavatrixFactsBundle) -> ExtractReport {
    let mut candidates: Vec<ApiCandidate> = bundle
        .entities
        .iter()
        .map(|e| entity_to_candidate(e, bundle))
        .collect();
    candidates.sort_by(|a, b| a.name.cmp(&b.name).then(a.line.cmp(&b.line)));

    let status = if candidates.is_empty() {
        "no_candidates".into()
    } else {
        "candidate_found_weavatrix".into()
    };

    let root = bundle
        .package_root
        .clone()
        .unwrap_or_else(|| format!("facts://{}", bundle.package_name));

    let mut notes = bundle.notes.clone();
    notes.push(format!(
        "source={} schema={} entities={} revision={}",
        bundle.source,
        bundle.schema_version,
        candidates.len(),
        bundle.revision.as_deref().unwrap_or("-")
    ));
    notes.push("ADR-0012: Weavatrix facts preferred over bootstrap AST when provided.".into());

    ExtractReport {
        root,
        package_name: bundle.package_name.clone(),
        candidates,
        status,
        files_ast: 0,
        files_line_fallback: 0,
        // extend extract report? keep as-is - status string carries provenance
    }
}

fn entity_to_candidate(e: &WeavatrixFactEntity, bundle: &WeavatrixFactsBundle) -> ApiCandidate {
    let kind = parse_kind(&e.kind);
    let mut notes = e.notes.clone();
    notes.push("extractor=weavatrix".into());
    let path = if e.path.is_empty() {
        "<weavatrix>".into()
    } else {
        e.path.clone()
    };
    // Prefer explicit entity_id; else stable synthetic id from package+path#line+name.
    let entity_id = e.entity_id.clone().or_else(|| {
        Some(format!(
            "{}:{}:{}#{}",
            bundle.package_name, path, e.line, e.name
        ))
    });
    ApiCandidate {
        kind,
        name: e.name.clone(),
        path,
        line: e.line,
        signature: if e.signature.is_empty() {
            e.name.clone()
        } else {
            e.signature.clone()
        },
        shape: CandidateShape {
            inputs: e.inputs.clone(),
            outputs: e.outputs.clone(),
            notes: notes.clone(),
        },
        extractor: "weavatrix".into(),
        module_path: e.module_path.clone(),
        source_entity_id: entity_id,
        source_revision: bundle.revision.clone(),
    }
}

fn parse_kind(s: &str) -> CandidateKind {
    match s.trim().to_ascii_lowercase().as_str() {
        "function" | "fn" | "method" => CandidateKind::Function,
        "struct" => CandidateKind::Struct,
        "enum" => CandidateKind::Enum,
        "trait" => CandidateKind::Trait,
        "module" | "mod" => CandidateKind::Module,
        _ => CandidateKind::Function,
    }
}

/// Export a bootstrap extract report as a Weavatrix-compatible facts bundle.
///
/// Useful to freeze AST extract into the interchange format, or for tests.
pub fn facts_from_extract(extract: &ExtractReport, source: &str) -> WeavatrixFactsBundle {
    let entities = extract
        .candidates
        .iter()
        .map(|c| WeavatrixFactEntity {
            kind: kind_label(c.kind).into(),
            name: c.name.clone(),
            entity_id: c.source_entity_id.clone().or_else(|| {
                Some(format!("{}:{}#{}", c.path, c.line, c.name))
            }),
            path: c.path.clone(),
            line: c.line,
            signature: c.signature.clone(),
            module_path: c.module_path.clone(),
            inputs: c.shape.inputs.clone(),
            outputs: c.shape.outputs.clone(),
            notes: {
                let mut n = c.shape.notes.clone();
                n.push(format!("bootstrap_extractor={}", c.extractor));
                n
            },
        })
        .collect();

    WeavatrixFactsBundle {
        schema_version: FACTS_SCHEMA_VERSION.into(),
        source: source.into(),
        package_name: extract.package_name.clone(),
        package_root: Some(extract.root.clone()),
        package_version: None,
        revision: None,
        entities,
        notes: vec![
            format!("exported_from_extract status={}", extract.status),
            "Interchange for Weavatrix → Forge; re-import with forge facts/match.".into(),
            "bootstrap-ast is fallback only — product path is Weavatrix facts (ADR-0012).".into(),
        ],
    }
}

fn kind_label(k: CandidateKind) -> &'static str {
    match k {
        CandidateKind::Function => "function",
        CandidateKind::Struct => "struct",
        CandidateKind::Enum => "enum",
        CandidateKind::Trait => "trait",
        CandidateKind::Module => "module",
    }
}

/// Write facts JSON to a path (pretty).
pub fn write_facts_file(
    bundle: &WeavatrixFactsBundle,
    path: impl AsRef<Path>,
) -> Result<(), ForgeError> {
    let path = path.as_ref();
    let text = serde_json::to_string_pretty(bundle)
        .map_err(|e| ForgeError::Parse(path.to_path_buf(), e.to_string()))?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| ForgeError::Io(parent.to_path_buf(), e))?;
        }
    }
    fs::write(path, text).map_err(|e| ForgeError::Io(path.to_path_buf(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bundle() -> WeavatrixFactsBundle {
        WeavatrixFactsBundle {
            schema_version: FACTS_SCHEMA_VERSION.into(),
            source: "weavatrix".into(),
            package_name: "demo_crate".into(),
            package_root: Some("/tmp/demo_crate".into()),
            package_version: Some("0.1.0".into()),
            revision: Some("rev-test-1".into()),
            entities: vec![WeavatrixFactEntity {
                kind: "function".into(),
                name: "parse".into(),
                entity_id: Some("wvx:demo_crate:fn:parse".into()),
                path: "src/lib.rs".into(),
                line: 10,
                signature: "pub fn parse(bytes: &[u8]) -> Result<Value, E>".into(),
                module_path: Some("lib".into()),
                inputs: vec!["bytes".into()],
                outputs: vec!["json_value".into()],
                notes: vec![],
            }],
            notes: vec![],
        }
    }

    #[test]
    fn facts_roundtrip_to_extract() {
        let b = sample_bundle();
        let extract = extract_from_facts(&b);
        assert_eq!(extract.package_name, "demo_crate");
        assert_eq!(extract.candidates.len(), 1);
        assert_eq!(extract.candidates[0].extractor, "weavatrix");
        assert_eq!(
            extract.candidates[0].source_entity_id.as_deref(),
            Some("wvx:demo_crate:fn:parse")
        );
        assert_eq!(
            extract.candidates[0].source_revision.as_deref(),
            Some("rev-test-1")
        );
        assert_eq!(extract.status, "candidate_found_weavatrix");
        let back = facts_from_extract(&extract, "bootstrap-export");
        assert_eq!(back.entities.len(), 1);
        assert_eq!(back.schema_version, FACTS_SCHEMA_VERSION);
        assert_eq!(
            back.entities[0].entity_id.as_deref(),
            Some("wvx:demo_crate:fn:parse")
        );
    }

    #[test]
    fn rejects_wrong_schema() {
        let mut b = sample_bundle();
        b.schema_version = "nope".into();
        assert!(validate_facts(&b).is_err());
    }
}
