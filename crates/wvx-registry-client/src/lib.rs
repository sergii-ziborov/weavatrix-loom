//! Read capabilities and implementations from a local registry directory.
//!
//! Expected layout (v0.1):
//! ```text
//! registry/
//!   capabilities/**/*.json
//!   implementations/**/*.json
//!   index/capabilities.json   (optional flat list)
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use wvx_ir::{Capability, Implementation};

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

impl LocalRegistry {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, RegistryError> {
        let root = root.into();
        if !root.is_dir() {
            return Err(RegistryError::MissingRoot(root));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn list_capabilities(&self) -> Result<Vec<Capability>, RegistryError> {
        let dir = self.root.join("capabilities");
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        read_json_dir(&dir)
    }

    pub fn list_implementations(&self) -> Result<Vec<Implementation>, RegistryError> {
        let dir = self.root.join("implementations");
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        read_json_dir(&dir)
    }

    pub fn find_capability(&self, id: &str, version: &str) -> Result<Option<Capability>, RegistryError> {
        Ok(self
            .list_capabilities()?
            .into_iter()
            .find(|c| c.id == id && c.version == version))
    }
}

fn read_json_dir<T: serde::de::DeserializeOwned>(dir: &Path) -> Result<Vec<T>, RegistryError> {
    let mut out = Vec::new();
    let entries = fs::read_dir(dir).map_err(|e| RegistryError::Io(dir.to_path_buf(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| RegistryError::Io(dir.to_path_buf(), e))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| RegistryError::Io(path.clone(), e))?;
        let value: T = serde_json::from_str(&text)
            .map_err(|e| RegistryError::Parse(path, e.to_string()))?;
        out.push(value);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn empty_registry() {
        let dir = tempfile_dir();
        let reg = LocalRegistry::open(&dir).unwrap();
        assert!(reg.list_capabilities().unwrap().is_empty());
    }

    #[test]
    fn loads_capability_json() {
        let dir = tempfile_dir();
        let cap_dir = dir.join("capabilities");
        fs::create_dir_all(&cap_dir).unwrap();
        let mut f = fs::File::create(cap_dir.join("parse.json")).unwrap();
        write!(
            f,
            r#"{{
              "id": "data.json.parse",
              "version": "1",
              "kind": "transform",
              "inputs": [{{"id": "bytes", "type": "bytes", "required": true}}],
              "outputs": [{{"id": "value", "type": "json_value", "required": true}}]
            }}"#
        )
        .unwrap();
        let reg = LocalRegistry::open(&dir).unwrap();
        let caps = reg.list_capabilities().unwrap();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].id, "data.json.parse");
    }

    fn tempfile_dir() -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "wvx-registry-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
