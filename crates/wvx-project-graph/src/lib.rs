//! In-memory project graph mutations (semantic ops; UI metadata is optional).

mod patch;

pub use patch::{
    apply_graph_patch, propose_json_pipeline_patch, validate_graph_patch, GraphOp, GraphPatch,
    PatchApplyResult, PatchError,
};

use thiserror::Error;
use wvx_ir::{Binding, Instance, PortPath, Project, UiPosition};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GraphError {
    #[error("instance `{0}` already exists")]
    DuplicateInstance(String),
    #[error("instance `{0}` not found")]
    MissingInstance(String),
    #[error("binding already exists: {0} → {1}")]
    DuplicateBinding(PortPath, PortPath),
    #[error("self-binding is not allowed on `{0}`")]
    SelfBinding(PortPath),
}

/// Add an instance to the project.
pub fn add_instance(project: &mut Project, instance: Instance) -> Result<(), GraphError> {
    if project.instance(&instance.id).is_some() {
        return Err(GraphError::DuplicateInstance(instance.id));
    }
    project.instances.push(instance);
    Ok(())
}

/// Remove an instance and any bindings that touch it.
pub fn remove_instance(project: &mut Project, instance_id: &str) -> Result<(), GraphError> {
    let before = project.instances.len();
    project.instances.retain(|i| i.id != instance_id);
    if project.instances.len() == before {
        return Err(GraphError::MissingInstance(instance_id.into()));
    }
    project
        .bindings
        .retain(|b| b.from.instance != instance_id && b.to.instance != instance_id);
    if project.entrypoint.as_deref() == Some(instance_id) {
        project.entrypoint = None;
    }
    Ok(())
}

/// Move UI coordinates only (compiler/runtime ignore this).
pub fn move_instance(
    project: &mut Project,
    instance_id: &str,
    position: UiPosition,
) -> Result<(), GraphError> {
    let instance = project
        .instances
        .iter_mut()
        .find(|i| i.id == instance_id)
        .ok_or_else(|| GraphError::MissingInstance(instance_id.into()))?;
    instance.ui = Some(position);
    Ok(())
}

/// Connect two ports. Does not type-check; call `wvx_validator` for that.
pub fn connect(project: &mut Project, from: PortPath, to: PortPath) -> Result<(), GraphError> {
    if from == to {
        return Err(GraphError::SelfBinding(from));
    }
    if project.instance(&from.instance).is_none() {
        return Err(GraphError::MissingInstance(from.instance));
    }
    if project.instance(&to.instance).is_none() {
        return Err(GraphError::MissingInstance(to.instance));
    }
    if project
        .bindings
        .iter()
        .any(|b| b.from == from && b.to == to)
    {
        return Err(GraphError::DuplicateBinding(from, to));
    }
    project.bindings.push(Binding { from, to });
    Ok(())
}

pub fn disconnect(project: &mut Project, from: &PortPath, to: &PortPath) -> bool {
    let before = project.bindings.len();
    project
        .bindings
        .retain(|b| !(b.from == *from && b.to == *to));
    project.bindings.len() != before
}

pub fn set_entrypoint(project: &mut Project, instance_id: Option<String>) -> Result<(), GraphError> {
    if let Some(ref id) = instance_id {
        if project.instance(id).is_none() {
            return Err(GraphError::MissingInstance(id.clone()));
        }
    }
    project.entrypoint = instance_id;
    Ok(())
}

pub fn select_implementation(
    project: &mut Project,
    instance_id: &str,
    implementation: Option<String>,
) -> Result<(), GraphError> {
    let instance = project
        .instances
        .iter_mut()
        .find(|i| i.id == instance_id)
        .ok_or_else(|| GraphError::MissingInstance(instance_id.into()))?;
    instance.implementation = implementation;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::{CapabilityRef, Project};

    fn sample_instance(id: &str) -> Instance {
        Instance {
            id: id.into(),
            capability: CapabilityRef::new("data.json.parse", "1"),
            implementation: None,
            config: Default::default(),
            ui: None,
        }
    }

    #[test]
    fn connect_and_remove() {
        let mut p = Project::new("t", "t");
        add_instance(&mut p, sample_instance("a")).unwrap();
        add_instance(&mut p, sample_instance("b")).unwrap();
        connect(
            &mut p,
            PortPath::new("a", "out"),
            PortPath::new("b", "in"),
        )
        .unwrap();
        assert_eq!(p.bindings.len(), 1);
        remove_instance(&mut p, "a").unwrap();
        assert!(p.bindings.is_empty());
        assert_eq!(p.instances.len(), 1);
    }
}
