//! GraphPatch — structured edits shared by Studio, CLI, MCP, and AI.
//!
//! AI may **propose** patches; only validated apply is authoritative.

use crate::{add_instance, connect, remove_instance, select_implementation, GraphError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use wvx_ir::{Capability, CapabilityRef, Instance, PortPath, Project, UiPosition};
use wvx_validator::{validate_project, ValidationReport};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum GraphOp {
    AddInstance {
        instance: Instance,
        #[serde(default)]
        capability: Option<Capability>,
    },
    RemoveInstance {
        id: String,
    },
    Connect {
        from: PortPath,
        to: PortPath,
    },
    Disconnect {
        from: PortPath,
        to: PortPath,
    },
    SelectImplementation {
        instance_id: String,
        implementation: Option<String>,
    },
    SetConfig {
        instance_id: String,
        config: BTreeMap<String, serde_json::Value>,
    },
    SetEntrypoint {
        instance_id: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphPatch {
    #[serde(default)]
    pub ops: Vec<GraphOp>,
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub unresolved: Vec<String>,
    /// When set, apply/validate require `project.revision == base_revision` (PATCH-001).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<u64>,
    /// Optional client/server patch id for tracing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchApplyResult {
    pub project: Project,
    pub validation: ValidationReport,
    pub applied_ops: usize,
    /// Project revision after apply (bumped on success).
    #[serde(default)]
    pub revision: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum PatchError {
    #[error(transparent)]
    Graph(#[from] GraphError),
    #[error("patch op {index}: {message}")]
    Op { index: usize, message: String },
    #[error("revision mismatch: patch base_revision={expected}, project.revision={actual}")]
    RevisionMismatch { expected: u64, actual: u64 },
}

/// Apply ops in order to a clone; returns project + validation.
///
/// When `patch.base_revision` is `Some(r)`, requires `project.revision == r`.
/// On success, `project.revision` is incremented.
pub fn apply_graph_patch(
    project: &Project,
    patch: &GraphPatch,
) -> Result<PatchApplyResult, PatchError> {
    if let Some(expected) = patch.base_revision {
        if project.revision != expected {
            return Err(PatchError::RevisionMismatch {
                expected,
                actual: project.revision,
            });
        }
    }
    let mut p = project.clone();
    for (index, op) in patch.ops.iter().enumerate() {
        apply_one(&mut p, op).map_err(|e| PatchError::Op {
            index,
            message: e.to_string(),
        })?;
    }
    p.bump_revision();
    let validation = validate_project(&p);
    let revision = p.revision;
    Ok(PatchApplyResult {
        project: p,
        validation,
        applied_ops: patch.ops.len(),
        revision,
    })
}

/// Validate without returning the mutated project as success if invalid —
/// still returns the applied project so UI can show ghost state.
pub fn validate_graph_patch(
    project: &Project,
    patch: &GraphPatch,
) -> Result<PatchApplyResult, PatchError> {
    apply_graph_patch(project, patch)
}

fn apply_one(project: &mut Project, op: &GraphOp) -> Result<(), GraphError> {
    match op {
        GraphOp::AddInstance {
            instance,
            capability,
        } => {
            if let Some(cap) = capability {
                if project.capability_for(&CapabilityRef::new(&cap.id, &cap.version)).is_none() {
                    project.capabilities.push(cap.clone());
                }
            }
            add_instance(project, instance.clone())
        }
        GraphOp::RemoveInstance { id } => remove_instance(project, id),
        GraphOp::Connect { from, to } => connect(project, from.clone(), to.clone()),
        GraphOp::Disconnect { from, to } => {
            project
                .bindings
                .retain(|b| !(b.from == *from && b.to == *to));
            Ok(())
        }
        GraphOp::SelectImplementation {
            instance_id,
            implementation,
        } => select_implementation(project, instance_id, implementation.clone()),
        GraphOp::SetConfig {
            instance_id,
            config,
        } => {
            let inst = project
                .instances
                .iter_mut()
                .find(|i| i.id == *instance_id)
                .ok_or_else(|| GraphError::MissingInstance(instance_id.clone()))?;
            inst.config = config.clone();
            Ok(())
        }
        GraphOp::SetEntrypoint { instance_id } => {
            if let Some(id) = instance_id {
                if project.instance(id).is_none() {
                    return Err(GraphError::MissingInstance(id.clone()));
                }
            }
            project.entrypoint = instance_id.clone();
            Ok(())
        }
    }
}

/// Desired pilot nodes: (instance_id, capability_id, default_impl, x).
const PILOT_NODES: &[(&str, &str, Option<&str>, f64)] = &[
    ("input", "io.input.bytes", None, 40.0),
    (
        "parse",
        "data.json.parse",
        Some("serde-json.parse-owned@1"),
        280.0,
    ),
    (
        "path_set",
        "data.json.path_set",
        Some("wvx.reference.path-set@1"),
        520.0,
    ),
    (
        "serialize",
        "data.json.serialize",
        Some("serde-json.serialize@1"),
        760.0,
    ),
    ("output", "io.output.bytes", None, 1000.0),
];

const PILOT_EDGES: &[(&str, &str, &str, &str)] = &[
    ("input", "bytes", "parse", "bytes"),
    ("parse", "value", "path_set", "value"),
    ("path_set", "value", "serialize", "value"),
    ("serialize", "bytes", "output", "bytes"),
];

/// Rule-based pilot proposal against an **empty** base (full recipe).
///
/// Prefer [`propose_json_pipeline_patch_relative`] when a project already exists.
pub fn propose_json_pipeline_patch(capabilities: &[Capability]) -> GraphPatch {
    propose_json_pipeline_patch_relative(&Project::new("empty", "Empty"), capabilities)
}

/// Rule-based pilot proposal **relative to the current project**.
///
/// Only emits ops needed to reach the v0.1 JSON pilot pipeline:
/// - skips instances / bindings that already match
/// - adds missing nodes (with capability contracts when available)
/// - connects missing edges
/// - sets entrypoint / path_set config / default impls when absent
///
/// If the project already matches the pilot recipe, returns an empty op list
/// (not an error) with a clear rationale.
pub fn propose_json_pipeline_patch_relative(
    project: &Project,
    capabilities: &[Capability],
) -> GraphPatch {
    let caps = if capabilities.is_empty() {
        // Prefer contracts already embedded on the project, else pilot defaults.
        if project.capabilities.is_empty() {
            pilot_capabilities()
        } else {
            let mut merged = project.capabilities.clone();
            for c in pilot_capabilities() {
                if project
                    .capability_for(&CapabilityRef::new(&c.id, &c.version))
                    .is_none()
                    && !merged.iter().any(|m| m.id == c.id && m.version == c.version)
                {
                    merged.push(c);
                }
            }
            // Also allow registry list via empty→caller; here we only have project.
            if merged.len() < 5 {
                for c in pilot_capabilities() {
                    if !merged.iter().any(|m| m.id == c.id && m.version == c.version) {
                        merged.push(c);
                    }
                }
            }
            merged
        }
    } else {
        capabilities.to_vec()
    };

    let find = |id: &str| {
        caps.iter()
            .find(|c| c.id == id && c.version == "1")
            .cloned()
            .or_else(|| {
                project
                    .capabilities
                    .iter()
                    .find(|c| c.id == id && c.version == "1")
                    .cloned()
            })
    };

    let mut ops = Vec::new();
    let mut unresolved = Vec::new();
    let mut will_exist: std::collections::BTreeSet<String> = project
        .instances
        .iter()
        .map(|i| i.id.clone())
        .collect();

    // --- instances ---
    for (id, cap_id, impl_id, x) in PILOT_NODES {
        if let Some(existing) = project.instance(id) {
            // Id taken: only fix impl/config if same capability family.
            if existing.capability.id != *cap_id || existing.capability.version != "1" {
                unresolved.push(format!(
                    "instance `{id}` exists with capability `{}` (want `{cap_id}@1`); not overwritten",
                    existing.capability.as_key()
                ));
                continue;
            }
            if let Some(want_impl) = impl_id {
                let cur = existing.implementation.as_deref().unwrap_or("");
                if cur.is_empty() {
                    ops.push(GraphOp::SelectImplementation {
                        instance_id: (*id).into(),
                        implementation: Some((*want_impl).into()),
                    });
                }
            }
            if *id == "path_set" {
                let path_ok = existing
                    .config
                    .get("path")
                    .and_then(|v| v.as_str())
                    .is_some();
                let val_ok = existing.config.get("value").is_some();
                if !path_ok || !val_ok {
                    let mut c = existing.config.clone();
                    c.entry("path".into())
                        .or_insert_with(|| serde_json::json!("/tag"));
                    c.entry("value".into())
                        .or_insert_with(|| serde_json::json!("loom"));
                    ops.push(GraphOp::SetConfig {
                        instance_id: (*id).into(),
                        config: c,
                    });
                }
            }
            continue;
        }

        // Fresh instance — recipe x on empty canvas; otherwise append to the right.
        let mut config = BTreeMap::new();
        if *id == "path_set" {
            config.insert("path".into(), serde_json::json!("/tag"));
            config.insert("value".into(), serde_json::json!("loom"));
        }
        let pending_adds = ops
            .iter()
            .filter(|o| matches!(o, GraphOp::AddInstance { .. }))
            .count();
        let layout_x = if project.instances.is_empty() {
            *x
        } else {
            let max_x = project
                .instances
                .iter()
                .filter_map(|i| i.ui.map(|u| u.x))
                .fold(40.0_f64, f64::max);
            max_x + 260.0 * (pending_adds as f64 + 1.0)
        };

        ops.push(GraphOp::AddInstance {
            instance: Instance {
                id: (*id).into(),
                capability: CapabilityRef::new(*cap_id, "1"),
                implementation: impl_id.map(str::to_string),
                config,
                ui: Some(UiPosition {
                    x: layout_x,
                    y: 120.0,
                }),
            },
            capability: find(cap_id),
        });
        will_exist.insert((*id).into());
    }

    // --- bindings ---
    for (fi, fp, ti, tp) in PILOT_EDGES {
        let from = PortPath::new(*fi, *fp);
        let to = PortPath::new(*ti, *tp);
        let already = project
            .bindings
            .iter()
            .any(|b| b.from == from && b.to == to);
        if already {
            continue;
        }
        if !will_exist.contains(*fi) || !will_exist.contains(*ti) {
            unresolved.push(format!(
                "cannot connect {fi}.{fp} → {ti}.{tp}: missing endpoint instance"
            ));
            continue;
        }
        ops.push(GraphOp::Connect { from, to });
    }

    // --- entrypoint ---
    if project.entrypoint.as_deref() != Some("input") && will_exist.contains("input") {
        ops.push(GraphOp::SetEntrypoint {
            instance_id: Some("input".into()),
        });
    }

    if ops.is_empty() && unresolved.is_empty() {
        return GraphPatch {
            ops,
            rationale: "Relative propose: project already matches the JSON pilot pipeline (no ops)."
                .into(),
            unresolved: vec![
                "Choose parse implementation (serde-json vs reference)".into(),
                "Choose serialize implementation (compact vs pretty)".into(),
            ],
            ..Default::default()
        };
    }

    let added = ops
        .iter()
        .filter(|o| matches!(o, GraphOp::AddInstance { .. }))
        .count();
    let connected = ops
        .iter()
        .filter(|o| matches!(o, GraphOp::Connect { .. }))
        .count();
    let rationale = if project.instances.is_empty() {
        "Propose the v0.1 JSON pilot pipeline: Input → Parse → PathSet → Serialize → Output."
            .to_string()
    } else {
        format!(
            "Relative propose against current project: +{added} instance(s), +{connected} binding(s) toward JSON pilot pipeline."
        )
    };

    if unresolved.is_empty() {
        unresolved.push("Choose parse implementation (serde-json vs reference)".into());
        unresolved.push("Choose serialize implementation (compact vs pretty)".into());
    }

    GraphPatch {
        ops,
        rationale,
        unresolved,
        ..Default::default()
    }
}

fn pilot_capabilities() -> Vec<Capability> {
    use wvx_ir::PortSpec;
    use wvx_types::TypeRef;
    vec![
        Capability {
            id: "io.input.bytes".into(),
            version: "1".into(),
            kind: "io".into(),
            inputs: vec![],
            outputs: vec![PortSpec {
                id: "bytes".into(),
                ty: TypeRef::Bytes,
                required: true,
            }],
            errors: vec![],
            effects: vec![],
        },
        Capability {
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
            errors: vec![],
            effects: vec![],
        },
        Capability {
            id: "data.json.path_set".into(),
            version: "1".into(),
            kind: "transform".into(),
            inputs: vec![PortSpec {
                id: "value".into(),
                ty: TypeRef::JsonValue,
                required: true,
            }],
            outputs: vec![PortSpec {
                id: "value".into(),
                ty: TypeRef::JsonValue,
                required: true,
            }],
            errors: vec![],
            effects: vec![],
        },
        Capability {
            id: "data.json.serialize".into(),
            version: "1".into(),
            kind: "transform".into(),
            inputs: vec![PortSpec {
                id: "value".into(),
                ty: TypeRef::JsonValue,
                required: true,
            }],
            outputs: vec![PortSpec {
                id: "bytes".into(),
                ty: TypeRef::Bytes,
                required: true,
            }],
            errors: vec![],
            effects: vec![],
        },
        Capability {
            id: "io.output.bytes".into(),
            version: "1".into(),
            kind: "io".into(),
            inputs: vec![PortSpec {
                id: "bytes".into(),
                ty: TypeRef::Bytes,
                required: true,
            }],
            outputs: vec![],
            errors: vec![],
            effects: vec![],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::PROJECT_SCHEMA_VERSION;

    #[test]
    fn propose_and_apply_pilot() {
        let mut project = Project::new("empty", "Empty");
        project.schema_version = PROJECT_SCHEMA_VERSION.into();
        let patch = propose_json_pipeline_patch(&[]);
        let result = apply_graph_patch(&project, &patch).unwrap();
        assert!(result.validation.is_ok(), "{:?}", result.validation.diagnostics);
        assert_eq!(result.project.instances.len(), 5);
        assert_eq!(result.project.bindings.len(), 4);
    }

    #[test]
    fn relative_propose_is_noop_on_full_pilot() {
        let mut project = Project::new("empty", "Empty");
        project.schema_version = PROJECT_SCHEMA_VERSION.into();
        let full = propose_json_pipeline_patch(&[]);
        let applied = apply_graph_patch(&project, &full).unwrap();
        let relative = propose_json_pipeline_patch_relative(&applied.project, &[]);
        assert!(
            relative.ops.is_empty(),
            "expected no ops, got {:?}",
            relative.ops
        );
        assert!(relative.rationale.contains("already matches"));
    }

    #[test]
    fn relative_propose_fills_missing_tail() {
        let mut project = Project::new("partial", "Partial");
        project.schema_version = PROJECT_SCHEMA_VERSION.into();
        // Only input node
        project.instances.push(Instance {
            id: "input".into(),
            capability: CapabilityRef::new("io.input.bytes", "1"),
            implementation: None,
            config: BTreeMap::new(),
            ui: Some(UiPosition { x: 40.0, y: 120.0 }),
        });
        project.capabilities = pilot_capabilities();
        project.entrypoint = Some("input".into());

        let patch = propose_json_pipeline_patch_relative(&project, &[]);
        assert!(
            patch.ops.iter().any(|o| matches!(
                o,
                GraphOp::AddInstance {
                    instance: Instance { id, .. },
                    ..
                } if id == "parse"
            )),
            "should add parse: {:?}",
            patch.ops
        );
        // Must not re-add input
        assert!(
            !patch.ops.iter().any(|o| matches!(
                o,
                GraphOp::AddInstance {
                    instance: Instance { id, .. },
                    ..
                } if id == "input"
            )),
            "must not re-add input"
        );

        let result = apply_graph_patch(&project, &patch).unwrap();
        assert!(result.validation.is_ok(), "{:?}", result.validation.diagnostics);
        assert_eq!(result.project.instances.len(), 5);
        assert_eq!(result.project.bindings.len(), 4);
    }

    #[test]
    fn relative_propose_adds_missing_binding_only() {
        let mut project = Project::new("p", "P");
        project.schema_version = PROJECT_SCHEMA_VERSION.into();
        let full = propose_json_pipeline_patch(&[]);
        let mut applied = apply_graph_patch(&project, &full).unwrap().project;
        // Drop one binding
        applied.bindings.retain(|b| {
            !(b.from.instance == "parse" && b.to.instance == "path_set")
        });
        let patch = propose_json_pipeline_patch_relative(&applied, &[]);
        assert_eq!(patch.ops.len(), 1, "ops={:?}", patch.ops);
        assert!(matches!(
            &patch.ops[0],
            GraphOp::Connect {
                from,
                to
            } if from.instance == "parse" && to.instance == "path_set"
        ));
    }
}
