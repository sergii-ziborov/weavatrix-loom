//! Read capabilities and implementations from a local registry directory.
//!
//! Expected layout (v0.1):
//! ```text
//! registry-dev/
//!   capabilities/**/*.json
//!   implementations/**/*.json
//!   index/capabilities.json
//!   index/implementations.json
//! ```

pub mod admission;
pub mod admit;
pub mod provenance;

pub use admission::{
    audit_implementations, check_implementation, justified_status, AdmissionReport,
    ImplementationAdmission,
};
pub use admit::{admit_implementation, AdmitRequest, AdmitResult};
pub use provenance::{
    provenance_from_impl, provenance_path, read_provenance, write_provenance, HumanReview,
    ProvenanceRecord,
};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use wvx_ir::{Capability, CapabilityRef, Implementation, Project};

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("registry path does not exist: {0}")]
    MissingRoot(PathBuf),
    #[error("io error at {0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("parse error at {0}: {1}")]
    Parse(PathBuf, String),
}

#[derive(Debug, Clone)]
pub struct LocalRegistry {
    root: PathBuf,
}

/// Compact search hit for CLI/MCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityHit {
    pub key: String,
    pub id: String,
    pub version: String,
    pub kind: String,
    pub implementation_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationHit {
    pub full_id: String,
    pub capability: String,
    pub source_kind: String,
    pub package: String,
    pub adapter: Option<String>,
    /// Lifecycle label: inventory_only | candidate | conformant | admitted
    #[serde(default)]
    pub status: String,
    /// Discrete evidence axes (build, conformance, …) → pass|fail|absent|unknown
    #[serde(default)]
    pub evidence: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Map a full implementation manifest to a list/search hit (includes status + evidence).
pub fn hit_from_implementation(i: Implementation) -> ImplementationHit {
    use wvx_ir::AxisFact;
    let mut evidence = BTreeMap::new();
    let push = |m: &mut BTreeMap<String, String>, k: &str, v: AxisFact| {
        m.insert(k.into(), v.as_str().into());
    };
    push(&mut evidence, "build", i.evidence.build);
    push(&mut evidence, "conformance", i.evidence.conformance);
    push(&mut evidence, "benchmark", i.evidence.benchmark);
    push(&mut evidence, "license", i.evidence.license);
    push(&mut evidence, "security", i.evidence.security);
    ImplementationHit {
        full_id: i.full_id(),
        capability: i.capability.as_key(),
        source_kind: i.source.kind,
        package: i.source.package,
        adapter: i.adapter.map(|a| a.crate_name),
        status: i.status.as_str().into(),
        evidence,
        notes: i.notes,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySummary {
    pub root: String,
    pub capabilities: usize,
    pub implementations: usize,
}

/// Result of installing one Forge draft as a local registry **candidate**.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallCandidateResult {
    pub implementation_id: String,
    pub capability_key: String,
    /// Set when a new capability file was written (new_proposal path).
    pub capability_written: Option<String>,
    pub path: String,
}

impl LocalRegistry {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, RegistryError> {
        let root = root.into();
        if !root.is_dir() {
            return Err(RegistryError::MissingRoot(root));
        }
        Ok(Self { root })
    }

    /// Resolve registry path from `WVX_REGISTRY`, else `./registry-dev` if present.
    pub fn open_default() -> Result<Self, RegistryError> {
        if let Ok(path) = std::env::var("WVX_REGISTRY") {
            return Self::open(path);
        }
        let cwd = std::env::current_dir().map_err(|e| {
            RegistryError::Io(PathBuf::from("."), e)
        })?;
        let candidate = cwd.join("registry-dev");
        if candidate.is_dir() {
            return Self::open(candidate);
        }
        // Walk up a few parents (repo root when cwd is a crate).
        let mut dir = cwd.as_path();
        for _ in 0..4 {
            if let Some(parent) = dir.parent() {
                let c = parent.join("registry-dev");
                if c.is_dir() {
                    return Self::open(c);
                }
                dir = parent;
            }
        }
        Err(RegistryError::MissingRoot(candidate))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn summary(&self) -> Result<RegistrySummary, RegistryError> {
        Ok(RegistrySummary {
            root: self.root.display().to_string(),
            capabilities: self.list_capabilities()?.len(),
            implementations: self.list_implementations()?.len(),
        })
    }

    pub fn list_capabilities(&self) -> Result<Vec<Capability>, RegistryError> {
        let dir = self.root.join("capabilities");
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut caps: Vec<Capability> = read_json_tree(&dir)?;
        caps.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.version.cmp(&b.version)));
        Ok(caps)
    }

    pub fn list_implementations(&self) -> Result<Vec<Implementation>, RegistryError> {
        let dir = self.root.join("implementations");
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut impls: Vec<Implementation> = read_json_tree(&dir)?;
        impls.sort_by(|a, b| a.full_id().cmp(&b.full_id()));
        Ok(impls)
    }

    pub fn find_capability(
        &self,
        id: &str,
        version: &str,
    ) -> Result<Option<Capability>, RegistryError> {
        Ok(self
            .list_capabilities()?
            .into_iter()
            .find(|c| c.id == id && c.version == version))
    }

    pub fn find_capability_key(&self, key: &str) -> Result<Option<Capability>, RegistryError> {
        let (id, version) = split_key(key);
        self.find_capability(id, version)
    }

    pub fn find_implementation(&self, full_id: &str) -> Result<Option<Implementation>, RegistryError> {
        Ok(self
            .list_implementations()?
            .into_iter()
            .find(|i| i.full_id() == full_id || format!("{}@{}", i.id, i.version) == full_id))
    }

    pub fn implementations_for_capability(
        &self,
        capability_key: &str,
    ) -> Result<Vec<Implementation>, RegistryError> {
        let (id, version) = split_key(capability_key);
        Ok(self
            .list_implementations()?
            .into_iter()
            .filter(|i| i.capability.id == id && i.capability.version == version)
            .collect())
    }

    pub fn search_capabilities(&self, query: &str) -> Result<Vec<CapabilityHit>, RegistryError> {
        // Tokenized AND search: "json parse" matches data.json.parse.
        // Also matches kind, port ids, and type labels (json_value / bytes / …).
        let tokens: Vec<String> = query
            .split(|c: char| c.is_whitespace() || c == ',' || c == '|')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| t.to_ascii_lowercase())
            .collect();
        let impls = self.list_implementations()?;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for imp in &impls {
            *counts
                .entry(imp.capability.as_key())
                .or_insert(0) += 1;
        }
        let mut hits = Vec::new();
        for cap in self.list_capabilities()? {
            let key = cap.as_ref_key().as_key();
            let mut hay = format!(
                "{} {} {} {}",
                key.to_ascii_lowercase(),
                cap.id.to_ascii_lowercase(),
                cap.kind.to_ascii_lowercase(),
                cap.version.to_ascii_lowercase()
            );
            for p in cap.inputs.iter().chain(cap.outputs.iter()) {
                hay.push(' ');
                hay.push_str(&p.id.to_ascii_lowercase());
                hay.push(' ');
                hay.push_str(&p.ty.to_string().to_ascii_lowercase());
                // also snake form of type (json.value → json_value)
                hay.push(' ');
                hay.push_str(&p.ty.to_string().to_ascii_lowercase().replace('.', "_"));
            }
            let ok = tokens.is_empty() || tokens.iter().all(|t| hay.contains(t));
            if ok {
                hits.push(CapabilityHit {
                    key: key.clone(),
                    id: cap.id,
                    version: cap.version,
                    kind: cap.kind,
                    implementation_count: *counts.get(&key).unwrap_or(&0),
                });
            }
        }
        // Prefer more specific id matches first when query non-empty.
        if !tokens.is_empty() {
            hits.sort_by(|a, b| {
                let score = |h: &CapabilityHit| {
                    let id = h.id.to_ascii_lowercase();
                    tokens
                        .iter()
                        .map(|t| {
                            if id == *t {
                                0
                            } else if id.ends_with(t.as_str()) {
                                1
                            } else if id.contains(t.as_str()) {
                                2
                            } else {
                                3
                            }
                        })
                        .sum::<i32>()
                };
                score(a).cmp(&score(b)).then_with(|| a.id.cmp(&b.id))
            });
        } else {
            hits.sort_by(|a, b| a.id.cmp(&b.id));
        }
        Ok(hits)
    }

    pub fn search_implementations(&self, query: &str) -> Result<Vec<ImplementationHit>, RegistryError> {
        let q = query.trim().to_ascii_lowercase();
        Ok(self
            .list_implementations()?
            .into_iter()
            .filter(|i| {
                if q.is_empty() {
                    return true;
                }
                let full = i.full_id().to_ascii_lowercase();
                let cap = i.capability.as_key().to_ascii_lowercase();
                full.contains(&q)
                    || cap.contains(&q)
                    || i.source.package.to_ascii_lowercase().contains(&q)
                    || i.source.kind.to_ascii_lowercase().contains(&q)
            })
            .map(hit_from_implementation)
            .collect())
    }

    /// Audit lifecycle labels vs evidence (overclaim detection). See [`admission`].
    pub fn audit_admission(&self) -> Result<AdmissionReport, RegistryError> {
        let impls = self.list_implementations()?;
        Ok(audit_implementations(&impls))
    }

    /// Install a Forge draft as **candidate** implementation (never admitted).
    ///
    /// - Writes `implementations/{full_id}.json` with `status: candidate`.
    /// - Optionally writes a new capability file when `write_capability` is true
    ///   and the capability is not already present (for `new_proposal` drafts).
    /// - Does **not** auto-admit or set conformance evidence.
    pub fn install_forge_draft_candidate(
        &self,
        capability_json: &str,
        implementation_json: &str,
        write_capability_if_new: bool,
    ) -> Result<InstallCandidateResult, RegistryError> {
        let mut impl_val: serde_json::Value = serde_json::from_str(implementation_json)
            .map_err(|e| RegistryError::Parse(self.root.join("implementation.json"), e.to_string()))?;
        // Promote inventory_only → candidate for local registry listing.
        if let Some(obj) = impl_val.as_object_mut() {
            obj.insert("status".into(), serde_json::json!("candidate"));
            let notes_owned = obj
                .get("notes")
                .and_then(|n| n.as_str())
                .map(str::to_owned);
            if let Some(notes) = notes_owned {
                if !notes.contains("candidate") {
                    obj.insert(
                        "notes".into(),
                        serde_json::json!(format!(
                            "{notes} Registered as candidate via Forge (not admitted)."
                        )),
                    );
                }
            }
        }
        let imp: Implementation = serde_json::from_value(impl_val.clone()).map_err(|e| {
            RegistryError::Parse(self.root.join("implementation.json"), e.to_string())
        })?;
        let full_id = imp.full_id();
        let impl_path = self
            .root
            .join("implementations")
            .join(format!("{full_id}.json"));
        fs::create_dir_all(impl_path.parent().unwrap_or(self.root.as_path()))
            .map_err(|e| RegistryError::Io(impl_path.clone(), e))?;
        let pretty = serde_json::to_string_pretty(&impl_val)
            .map_err(|e| RegistryError::Parse(impl_path.clone(), e.to_string()))?;
        fs::write(&impl_path, pretty).map_err(|e| RegistryError::Io(impl_path.clone(), e))?;

        let mut cap_written = None;
        if write_capability_if_new {
            let mut cap_val: serde_json::Value = serde_json::from_str(capability_json)
                .map_err(|e| RegistryError::Parse(self.root.join("capability.json"), e.to_string()))?;
            // Drop forge-only annotations before persistence.
            if let Some(obj) = cap_val.as_object_mut() {
                obj.remove("_forge");
            }
            let cap: Capability = serde_json::from_value(cap_val.clone()).map_err(|e| {
                RegistryError::Parse(self.root.join("capability.json"), e.to_string())
            })?;
            let key = format!("{}@{}", cap.id, cap.version);
            if self.find_capability(&cap.id, &cap.version)?.is_none() {
                let cap_path = self
                    .root
                    .join("capabilities")
                    .join(format!("{key}.json"));
                fs::create_dir_all(cap_path.parent().unwrap_or(self.root.as_path()))
                    .map_err(|e| RegistryError::Io(cap_path.clone(), e))?;
                let pretty = serde_json::to_string_pretty(&cap_val)
                    .map_err(|e| RegistryError::Parse(cap_path.clone(), e.to_string()))?;
                fs::write(&cap_path, pretty).map_err(|e| RegistryError::Io(cap_path.clone(), e))?;
                cap_written = Some(key);
            }
        }

        Ok(InstallCandidateResult {
            implementation_id: full_id,
            capability_key: imp.capability.as_key(),
            capability_written: cap_written,
            path: impl_path.display().to_string(),
        })
    }

    /// Merge missing capability contracts from the registry into a project.
    pub fn hydrate_project_capabilities(&self, project: &mut Project) -> Result<usize, RegistryError> {
        let mut added = 0;
        let mut needed: Vec<CapabilityRef> = project
            .instances
            .iter()
            .map(|i| i.capability.clone())
            .collect();
        needed.sort_by(|a, b| a.as_key().cmp(&b.as_key()));
        needed.dedup_by(|a, b| a.as_key() == b.as_key());

        for cap_ref in needed {
            if project.capability_for(&cap_ref).is_some() {
                continue;
            }
            if let Some(cap) = self.find_capability(&cap_ref.id, &cap_ref.version)? {
                project.capabilities.push(cap);
                added += 1;
            }
        }
        Ok(added)
    }
}

fn split_key(key: &str) -> (&str, &str) {
    key.rsplit_once('@').unwrap_or((key, "1"))
}

fn read_json_tree<T: serde::de::DeserializeOwned>(dir: &Path) -> Result<Vec<T>, RegistryError> {
    let mut out = Vec::new();
    walk_json(dir, &mut out)?;
    Ok(out)
}

fn walk_json<T: serde::de::DeserializeOwned>(
    dir: &Path,
    out: &mut Vec<T>,
) -> Result<(), RegistryError> {
    let entries = fs::read_dir(dir).map_err(|e| RegistryError::Io(dir.to_path_buf(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| RegistryError::Io(dir.to_path_buf(), e))?;
        let path = entry.path();
        if path.is_dir() {
            walk_json(&path, out)?;
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| RegistryError::Io(path.clone(), e))?;
        let value: T =
            serde_json::from_str(&text).map_err(|e| RegistryError::Parse(path, e.to_string()))?;
        out.push(value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_dev() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry-dev")
    }

    #[test]
    fn loads_registry_dev() {
        let reg = LocalRegistry::open(registry_dev()).unwrap();
        let caps = reg.list_capabilities().unwrap();
        let impls = reg.list_implementations().unwrap();
        assert!(caps.len() >= 5, "expected pilot capabilities, got {}", caps.len());
        assert!(impls.len() >= 8, "expected pilot implementations, got {}", impls.len());
        let parse_impls = reg
            .implementations_for_capability("data.json.parse@1")
            .unwrap();
        assert!(parse_impls.len() >= 2);
        assert!(parse_impls.iter().any(|i| i.full_id() == "serde-json.parse-owned@1"));
        assert!(parse_impls
            .iter()
            .any(|i| i.full_id() == "wvx.reference.json-parse@1"));
    }

    #[test]
    fn search_json() {
        let reg = LocalRegistry::open(registry_dev()).unwrap();
        let hits = reg.search_capabilities("json").unwrap();
        assert!(hits.iter().any(|h| h.key.starts_with("data.json.")));
    }

    #[test]
    fn search_multi_token_and_port_type() {
        let reg = LocalRegistry::open(registry_dev()).unwrap();
        let parse = reg.search_capabilities("json parse").unwrap();
        assert_eq!(parse.len(), 1);
        assert_eq!(parse[0].id, "data.json.parse");
        let by_type = reg.search_capabilities("bytes").unwrap();
        assert!(by_type.iter().any(|h| h.id == "io.input.bytes"));
        assert!(by_type.iter().any(|h| h.id == "data.json.parse")); // has bytes port
    }

    #[test]
    fn hydrate_project() {
        let reg = LocalRegistry::open(registry_dev()).unwrap();
        let mut project = Project::new("t", "t");
        project.instances.push(wvx_ir::Instance {
            id: "p".into(),
            capability: CapabilityRef::new("data.json.parse", "1"),
            implementation: Some("serde-json.parse-owned@1".into()),
            config: Default::default(),
            ui: None,
        });
        let added = reg.hydrate_project_capabilities(&mut project).unwrap();
        assert_eq!(added, 1);
        assert!(project
            .capability_for(&CapabilityRef::new("data.json.parse", "1"))
            .is_some());
    }
}
