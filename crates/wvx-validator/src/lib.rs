//! Validation passes for WVX projects.

use serde::{Deserialize, Serialize};
use wvx_ir::{PortPath, Project};
use wvx_types::TypeRef;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    pub fn ok() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    pub fn is_ok(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }
}

fn error(code: &str, message: impl Into<String>, path: Option<String>) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        severity: Severity::Error,
        message: message.into(),
        path,
    }
}

/// Run structural + port type checks available in v0.1.
pub fn validate_project(project: &Project) -> ValidationReport {
    let mut diagnostics = Vec::new();

    if project.id.trim().is_empty() {
        diagnostics.push(error("project.empty_id", "Project id is required", None));
    }
    if project.name.trim().is_empty() {
        diagnostics.push(error("project.empty_name", "Project name is required", None));
    }
    if project.schema_version.trim().is_empty() {
        diagnostics.push(error(
            "project.empty_schema",
            "schema_version is required",
            None,
        ));
    }

    let mut seen_ids = std::collections::BTreeSet::new();
    for instance in &project.instances {
        if !seen_ids.insert(instance.id.clone()) {
            diagnostics.push(error(
                "instance.duplicate",
                format!("Duplicate instance id `{}`", instance.id),
                Some(instance.id.clone()),
            ));
        }
        if project.capability_for(&instance.capability).is_none() {
            diagnostics.push(error(
                "instance.unknown_capability",
                format!(
                    "Instance `{}` references unknown capability `{}`",
                    instance.id,
                    instance.capability.as_key()
                ),
                Some(instance.id.clone()),
            ));
        }
    }

    if let Some(entry) = &project.entrypoint {
        if project.instance(entry).is_none() {
            diagnostics.push(error(
                "entrypoint.missing",
                format!("Entrypoint `{entry}` does not exist"),
                Some(entry.clone()),
            ));
        }
    }

    for binding in &project.bindings {
        let from_ty = match port_type(project, &binding.from, true) {
            Ok(ty) => ty,
            Err(d) => {
                diagnostics.push(d);
                continue;
            }
        };
        let to_ty = match port_type(project, &binding.to, false) {
            Ok(ty) => ty,
            Err(d) => {
                diagnostics.push(d);
                continue;
            }
        };
        if !from_ty.is_compatible_with(&to_ty) {
            diagnostics.push(error(
                "binding.type_mismatch",
                format!(
                    "Type mismatch: {} produces `{from_ty}`, {} expects `{to_ty}`",
                    binding.from, binding.to
                ),
                Some(format!("{}→{}", binding.from, binding.to)),
            ));
        }
    }

    for instance in &project.instances {
        let Some(cap) = project.capability_for(&instance.capability) else {
            continue;
        };
        for input in &cap.inputs {
            if !input.required {
                continue;
            }
            let path = PortPath::new(&instance.id, &input.id);
            let bound = project.bindings.iter().any(|b| b.to == path);
            if !bound {
                diagnostics.push(error(
                    "input.unbound",
                    format!("Required input `{path}` has no binding"),
                    Some(path.to_string()),
                ));
            }
        }
    }

    ValidationReport { diagnostics }
}

/// Resolve a port type when we know whether we expect an input or output.
pub fn port_type(
    project: &Project,
    path: &PortPath,
    as_output: bool,
) -> Result<TypeRef, Diagnostic> {
    let Some(instance) = project.instance(&path.instance) else {
        return Err(error(
            "port.missing_instance",
            format!("Instance `{}` not found", path.instance),
            Some(path.to_string()),
        ));
    };
    let Some(cap) = project.capability_for(&instance.capability) else {
        return Err(error(
            "port.unknown_capability",
            format!("Unknown capability for `{}`", path.instance),
            Some(path.to_string()),
        ));
    };
    if as_output {
        if let Some(out) = cap.output(&path.port) {
            return Ok(out.ty.clone());
        }
        if cap.input(&path.port).is_some() {
            return Err(error(
                "port.from_not_output",
                format!("`{path}` is an input; binding sources must be outputs"),
                Some(path.to_string()),
            ));
        }
        return Err(error(
            "port.missing_output",
            format!("No output port `{}` on `{}`", path.port, path.instance),
            Some(path.to_string()),
        ));
    }
    if let Some(inp) = cap.input(&path.port) {
        return Ok(inp.ty.clone());
    }
    if cap.output(&path.port).is_some() {
        return Err(error(
            "port.to_not_input",
            format!("`{path}` is an output; binding targets must be inputs"),
            Some(path.to_string()),
        ));
    }
    Err(error(
        "port.missing_input",
        format!("No input port `{}` on `{}`", path.port, path.instance),
        Some(path.to_string()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::{
        Binding, Capability, CapabilityRef, Instance, PortSpec, Project, PROJECT_SCHEMA_VERSION,
    };
    use wvx_types::TypeRef;

    fn pilot() -> Project {
        let mut p = Project::new("pilot", "pilot");
        p.schema_version = PROJECT_SCHEMA_VERSION.into();
        p.capabilities.push(Capability {
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
        });
        p.capabilities.push(Capability {
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
        });
        p.instances.push(Instance {
            id: "input".into(),
            capability: CapabilityRef::new("io.input.bytes", "1"),
            implementation: None,
            config: Default::default(),
            ui: None,
        });
        p.instances.push(Instance {
            id: "parse".into(),
            capability: CapabilityRef::new("data.json.parse", "1"),
            implementation: None,
            config: Default::default(),
            ui: None,
        });
        p.bindings.push(Binding {
            from: PortPath::new("input", "bytes"),
            to: PortPath::new("parse", "bytes"),
        });
        p.entrypoint = Some("input".into());
        p
    }

    #[test]
    fn valid_pilot_fragment() {
        let report = validate_project(&pilot());
        assert!(report.is_ok(), "{:?}", report.diagnostics);
    }

    #[test]
    fn rejects_output_as_binding_target() {
        let mut p = pilot();
        p.bindings[0].to = PortPath::new("parse", "value");
        let report = validate_project(&p);
        assert!(!report.is_ok());
        assert!(report.diagnostics.iter().any(|d| d.code == "port.to_not_input"));
    }
}
