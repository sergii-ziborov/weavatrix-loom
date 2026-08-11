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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphPatch {
    pub ops: Vec<GraphOp>,
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchApplyResult {
    pub project: Project,
    pub validation: ValidationReport,
    pub applied_ops: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum PatchError {
    #[error(transparent)]
    Graph(#[from] GraphError),
    #[error("patch op {index}: {message}")]
    Op { index: usize, message: String },
}

/// Apply ops in order to a clone; returns project + validation.
pub fn apply_graph_patch(
    project: &Project,
    patch: &GraphPatch,
) -> Result<PatchApplyResult, PatchError> {
    let mut p = project.clone();
    for (index, op) in patch.ops.iter().enumerate() {
        apply_one(&mut p, op).map_err(|e| PatchError::Op {
            index,
            message: e.to_string(),
        })?;
    }
    let validation = validate_project(&p);
    Ok(PatchApplyResult {
        project: p,
        validation,
        applied_ops: patch.ops.len(),
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

/// Rule-based pilot proposal: empty graph → full JSON pipeline.
///
/// Capabilities should be supplied by the caller (registry). Falls back to
/// embedded pilot contracts if `capabilities` is empty.
pub fn propose_json_pipeline_patch(capabilities: &[Capability]) -> GraphPatch {
    let caps = if capabilities.is_empty() {
        pilot_capabilities()
    } else {
        capabilities.to_vec()
    };

    let find = |id: &str| {
        caps.iter()
            .find(|c| c.id == id && c.version == "1")
            .cloned()
    };

    let mut ops = Vec::new();
    let nodes = [
        ("input", "io.input.bytes", None, 40.0),
        ("parse", "data.json.parse", Some("serde-json.parse-owned@1"), 280.0),
        ("path_set", "data.json.path_set", Some("wvx.reference.path-set@1"), 520.0),
        ("serialize", "data.json.serialize", Some("serde-json.serialize@1"), 760.0),
        ("output", "io.output.bytes", None, 1000.0),
    ];

    for (id, cap_id, impl_id, x) in nodes {
        let cap = find(cap_id);
        ops.push(GraphOp::AddInstance {
            instance: Instance {
                id: id.into(),
                capability: CapabilityRef::new(cap_id, "1"),
                implementation: impl_id.map(str::to_string),
                config: if id == "path_set" {
                    let mut c = BTreeMap::new();
                    c.insert("path".into(), serde_json::json!("/tag"));
                    c.insert("value".into(), serde_json::json!("loom"));
                    c
                } else {
                    BTreeMap::new()
                },
                ui: Some(UiPosition { x, y: 120.0 }),
            },
            capability: cap,
        });
    }

    ops.push(GraphOp::Connect {
        from: PortPath::new("input", "bytes"),
        to: PortPath::new("parse", "bytes"),
    });
    ops.push(GraphOp::Connect {
        from: PortPath::new("parse", "value"),
        to: PortPath::new("path_set", "value"),
    });
    ops.push(GraphOp::Connect {
        from: PortPath::new("path_set", "value"),
        to: PortPath::new("serialize", "value"),
    });
    ops.push(GraphOp::Connect {
        from: PortPath::new("serialize", "bytes"),
        to: PortPath::new("output", "bytes"),
    });
    ops.push(GraphOp::SetEntrypoint {
        instance_id: Some("input".into()),
    });

    GraphPatch {
        ops,
        rationale: "Propose the v0.1 JSON pilot pipeline: Input → Parse → PathSet → Serialize → Output."
            .into(),
        unresolved: vec![
            "Choose parse implementation (serde-json vs reference)".into(),
            "Choose serialize implementation (compact vs pretty)".into(),
        ],
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
}
