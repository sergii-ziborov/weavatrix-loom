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

/// Typed microbench snapshot attached to an implementation (Gate E / requal).
///
/// Never a readiness %. Timings are host-dependent; use `ok` + fingerprint for policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkRecord {
    pub implementation_id: String,
    pub capability_key: String,
    pub iterations: u32,
    pub warmup: u32,
    /// Execution success for the pilot bench harness.
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mean_ns: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_fingerprint: Option<String>,
    /// Unix seconds when recorded.
    pub recorded_at_unix: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Typed evidence package for resolve / human review / requalification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub implementation_id: String,
    pub capability_key: String,
    pub lifecycle: LifecycleStatus,
    pub axes: ImplementationEvidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bench: Option<BenchmarkRecord>,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Target deployment / product constraints for the **resolver** (not a readiness %).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TargetProfile {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    /// Prefer pure-Rust crates when ranking.
    #[serde(default)]
    pub prefer_pure_rust: bool,
    /// Soft preference against unsafe/FFI adapters (notes-based pilot heuristic).
    #[serde(default)]
    pub prefer_no_unsafe: bool,
}

/// Policy knobs for explainable implementation selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolverPolicy {
    pub id: String,
    /// Reject when evidence.conformance is Fail (Absent/Unknown still allowed unless required).
    #[serde(default = "default_true")]
    pub require_conformance_pass: bool,
    #[serde(default)]
    pub require_build_pass: bool,
    /// If false, `candidate` / `inventory_only` are rejected.
    #[serde(default = "default_true")]
    pub allow_candidate: bool,
    /// Prefer these full ids when multiple remain (first match wins).
    #[serde(default)]
    pub prefer_impl_ids: Vec<String>,
}

impl Default for ResolverPolicy {
    fn default() -> Self {
        Self {
            id: "default".into(),
            require_conformance_pass: true,
            require_build_pass: false,
            allow_candidate: true,
            prefer_impl_ids: Vec::new(),
        }
    }
}

/// Explainable resolver decision for one capability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolveDecision {
    pub capability_key: String,
    pub policy_id: String,
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chosen: Option<String>,
    /// Ordered candidates that were considered (after filters).
    #[serde(default)]
    pub ranked: Vec<String>,
    /// Human-readable explanation lines (why chosen / why rejected).
    #[serde(default)]
    pub explanation: Vec<String>,
    /// (impl_id, reason) for hard rejects.
    #[serde(default)]
    pub rejected: Vec<ResolveRejection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveRejection {
    pub implementation_id: String,
    pub reason: String,
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
    /// Gate F SDK binding (emit template + crate dep). Absent = legacy pilot map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdk: Option<SdkBinding>,
}

/// Manifest-driven adapter binding for core-independent extensibility (ADR-0011).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SdkBinding {
    /// Compiler emit for static export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emit: Option<SdkEmit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SdkEmit {
    /// Cargo package name of the adapter crate.
    pub crate_name: String,
    /// Optional path dependency relative to export/workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crate_path: Option<String>,
    /// Call template with `{port_id}` placeholders, e.g.
    /// `wvx_adapter_external_demo::upper_parse({bytes}.as_slice())?`
    pub template: String,
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
    /// Monotonic revision for GraphPatch base checks (ADR-0004 / PATCH-001).
    #[serde(default)]
    pub revision: u64,
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
            revision: 0,
            instances: Vec::new(),
            bindings: Vec::new(),
            capabilities: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    /// Bump revision after a successful semantic mutation (GraphPatch apply).
    pub fn bump_revision(&mut self) {
        self.revision = self.revision.saturating_add(1);
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
