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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySummary {
    pub root: String,
    pub capabilities: usize,
    pub implementations: usize,
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
        let q = query.trim().to_ascii_lowercase();
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
            if q.is_empty()
                || key.to_ascii_lowercase().contains(&q)
                || cap.kind.to_ascii_lowercase().contains(&q)
                || cap.id.to_ascii_lowercase().contains(&q)
            {
                hits.push(CapabilityHit {
                    key: key.clone(),
                    id: cap.id,
                    version: cap.version,
                    kind: cap.kind,
                    implementation_count: *counts.get(&key).unwrap_or(&0),
                });
            }
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
            .map(|i| ImplementationHit {
                full_id: i.full_id(),
                capability: i.capability.as_key(),
                source_kind: i.source.kind,
                package: i.source.package,
                adapter: i.adapter.map(|a| a.crate_name),
            })
            .collect())
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
