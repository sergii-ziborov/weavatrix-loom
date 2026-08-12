//! WVX intermediate representation: capabilities, implementations, instances,
//! bindings, and project documents.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use wvx_types::TypeRef;

/// Schema version for project documents on the wire.
pub const PROJECT_SCHEMA_VERSION: &str = "wvx.project.v0.1";

/// Stable capability identity (`data.json.parse`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityId(pub String);

/// Capability + major contract version (`data.json.parse@1`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityRef {
    pub id: String,
    pub version: String,
}

impl CapabilityRef {
    pub fn new(id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
        }
    }

    pub fn as_key(&self) -> String {
        format!("{}@{}", self.id, self.version)
    }
}

/// Port direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortDirection {
    Input,
    Output,
}

/// Typed port on a capability contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortSpec {
    pub id: String,
    #[serde(rename = "type")]
    pub ty: TypeRef,
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

/// Abstract capability contract (what, not how).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub version: String,
    pub kind: String,
    pub inputs: Vec<PortSpec>,
    pub outputs: Vec<PortSpec>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub effects: Vec<String>,
}

impl Capability {
    pub fn input(&self, id: &str) -> Option<&PortSpec> {
        self.inputs.iter().find(|p| p.id == id)
    }

    pub fn output(&self, id: &str) -> Option<&PortSpec> {
        self.outputs.iter().find(|p| p.id == id)
    }

    pub fn as_ref_key(&self) -> CapabilityRef {
        CapabilityRef::new(&self.id, &self.version)
    }
}

/// Lifecycle label for an implementation (ADR-0008 — not a readiness %).
///
/// - `inventory_only` — Forge/scan only, no adapter contract yet  
/// - `candidate` — adapter exists; evidence incomplete  
/// - `conformant` — shared capability vectors pass  
/// - `admitted` — policy + multi-fact admission (not claimed in pilot)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStatus {
    InventoryOnly,
    #[default]
    Candidate,
    Conformant,
    Admitted,
}

impl LifecycleStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InventoryOnly => "inventory_only",
            Self::Candidate => "candidate",
            Self::Conformant => "conformant",
            Self::Admitted => "admitted",
        }
    }
}

/// Single evidence axis fact (build / conformance / …). Never a global score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AxisFact {
    Pass,
    Fail,
    #[default]
    Absent,
    Unknown,
}

impl AxisFact {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Absent => "absent",
            Self::Unknown => "unknown",
        }
    }
}

/// Discrete multi-fact evidence bundle (ADR-0007 / 0008).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImplementationEvidence {
    #[serde(default)]
    pub build: AxisFact,
    #[serde(default)]
    pub conformance: AxisFact,
    #[serde(default)]
    pub benchmark: AxisFact,
    #[serde(default)]
    pub license: AxisFact,
    #[serde(default)]
    pub security: AxisFact,
}

/// Concrete Rust implementation of a capability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Implementation {
    pub id: String,
    pub version: String,
    pub capability: CapabilityRef,
    #[serde(default)]
    pub source: ImplementationSource,
    #[serde(default)]
    pub adapter: Option<AdapterRef>,
    /// Lifecycle chip for UI / policy (defaults to `candidate` if omitted).
    #[serde(default)]
    pub status: LifecycleStatus,
    /// Per-axis facts; missing axes deserialize as `absent`.
    #[serde(default)]
    pub evidence: ImplementationEvidence,
    /// Free-form note (e.g. "pilot only").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Implementation {
    /// Full registry key: `id@version` (e.g. `serde-json.parse-owned@1`).
    pub fn full_id(&self) -> String {
        format!("{}@{}", self.id, self.version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImplementationSource {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub package: String,
    #[serde(default)]
    pub package_version: String,
    /// Optional extra provenance text for humans/tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterRef {
    pub crate_name: String,
    #[serde(default = "default_native")]
    pub execution: String,
}

fn default_native() -> String {
    "native-rust".into()
}

/// UI-only placement (ignored by compiler semantics).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiPosition {
    pub x: f64,
    pub y: f64,
}

/// Placed capability instance in a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub capability: CapabilityRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation: Option<String>,
    #[serde(default)]
    pub config: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiPosition>,
}

/// Validated connection between instance ports: `instance.port`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub from: PortPath,
    pub to: PortPath,
}

/// `instance_id.port_id`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortPath {
    pub instance: String,
    pub port: String,
}

impl PortPath {
    pub fn new(instance: impl Into<String>, port: impl Into<String>) -> Self {
        Self {
            instance: instance.into(),
            port: port.into(),
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let (instance, port) = raw.rsplit_once('.')?;
        if instance.is_empty() || port.is_empty() {
            return None;
        }
        Some(Self::new(instance, port))
    }
}

impl std::fmt::Display for PortPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.instance, self.port)
    }
}

/// A Loom project document (WVX).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub schema_version: String,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub instances: Vec<Instance>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
    /// Capability contracts embedded or resolved for offline validation.
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl Project {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            schema_version: PROJECT_SCHEMA_VERSION.into(),
            id: id.into(),
            name: name.into(),
            entrypoint: None,
            instances: Vec::new(),
            bindings: Vec::new(),
            capabilities: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn instance(&self, id: &str) -> Option<&Instance> {
        self.instances.iter().find(|i| i.id == id)
    }

    pub fn capability_for(&self, cap: &CapabilityRef) -> Option<&Capability> {
        self.capabilities
            .iter()
            .find(|c| c.id == cap.id && c.version == cap.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_types::TypeRef;

    #[test]
    fn port_path_roundtrip() {
        let p = PortPath::parse("parse-request.bytes").unwrap();
        assert_eq!(p.instance, "parse-request");
        assert_eq!(p.port, "bytes");
        assert_eq!(p.to_string(), "parse-request.bytes");
    }

    #[test]
    fn project_json_smoke() {
        let mut project = Project::new("pilot", "JSON pilot");
        project.capabilities.push(Capability {
            id: "data.json.parse".into(),
            version: "1".into(),
            kind: "transform".into(),
            inputs: vec![PortSpec {
                id: "bytes".into(),
                ty: TypeRef::Bytes,
                required: true,
            }],
            outputs: vec![PortSpec {
                id: "value".into(),
                ty: TypeRef::JsonValue,
                required: true,
            }],
            errors: vec!["invalid-syntax".into()],
            effects: vec![],
        });
        let json = serde_json::to_string_pretty(&project).unwrap();
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back.capabilities.len(), 1);
    }
}
