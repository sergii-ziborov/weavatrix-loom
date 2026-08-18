//! Validation passes for WVX projects (Milestone 2 — Safe Semantic Core).
//!
//! Passes (in order):
//! schema · unique capabilities/ports/instances · entrypoint · bindings
//! (types + cardinality) · cycles · impl compatibility · config · outputs ·
//! compiler_profile · policy.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use wvx_ir::{Implementation, LifecycleStatus, PortPath, Project, PROJECT_SCHEMA_VERSION};
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
    /// Pass that emitted this diagnostic (M2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub diagnostics: Vec<Diagnostic>,
    /// Names of passes that ran (M2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passes_run: Vec<String>,
}

impl ValidationReport {
    pub fn ok() -> Self {
        Self {
            diagnostics: Vec::new(),
            passes_run: Vec::new(),
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

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }
}

/// Options for policy-aware / registry-aware validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateOptions {
    /// Known implementations (for impl-compat + lifecycle policy).
    #[serde(default)]
    pub implementations: Vec<Implementation>,
    /// Require `schema_version == PROJECT_SCHEMA_VERSION` (default true).
    #[serde(default = "default_true")]
    pub strict_schema: bool,
    /// Reject selected impls whose lifecycle is candidate/inventory_only.
    #[serde(default)]
    pub require_release_lifecycle: bool,
    /// If set and non-empty, `metadata.compiler_profile` must be one of these.
    #[serde(default)]
    pub allowed_compiler_profiles: Vec<String>,
    /// When implementations are provided, treat unknown selected impl ids as errors
    /// (default: warning only).
    #[serde(default)]
    pub require_known_implementation: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ValidateOptions {
    fn default() -> Self {
        Self::structural()
    }
}

impl ValidateOptions {
    /// Structural validation only (strict schema, no registry/policy).
    pub fn structural() -> Self {
        Self {
            implementations: Vec::new(),
            strict_schema: true,
            require_release_lifecycle: false,
            allowed_compiler_profiles: vec!["dev".into(), "release".into(), "check".into()],
            require_known_implementation: false,
        }
    }

    /// Release-oriented policy: no candidates, known impls preferred.
    pub fn release() -> Self {
        Self {
            implementations: Vec::new(),
            strict_schema: true,
            require_release_lifecycle: true,
            allowed_compiler_profiles: vec!["release".into(), "check".into()],
            require_known_implementation: true,
        }
    }
}

fn error(pass: &str, code: &str, message: impl Into<String>, path: Option<String>) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        severity: Severity::Error,
        message: message.into(),
        path,
        pass: Some(pass.into()),
    }
}

fn warning(pass: &str, code: &str, message: impl Into<String>, path: Option<String>) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        severity: Severity::Warning,
        message: message.into(),
        path,
        pass: Some(pass.into()),
    }
}

/// Run structural + type checks with default options (strict schema).
pub fn validate_project(project: &Project) -> ValidationReport {
    validate_project_with(project, &ValidateOptions::structural())
}

/// Run all M2 validation passes with explicit options.
pub fn validate_project_with(project: &Project, opts: &ValidateOptions) -> ValidationReport {
    let mut diagnostics = Vec::new();
    let mut passes_run = Vec::new();

    let mut run = |name: &str, f: fn(&Project, &ValidateOptions, &mut Vec<Diagnostic>)| {
        passes_run.push(name.into());
        f(project, opts, &mut diagnostics);
    };

    run("schema", pass_schema);
    run("unique_capabilities", pass_unique_capabilities);
    run("unique_ports", pass_unique_ports);
    run("unique_instances", pass_unique_instances);
    run("entrypoint", pass_entrypoint);
    run("bindings", pass_bindings);
    run("cycles", pass_cycles);
    run("impl_compatibility", pass_impl_compatibility);
    run("config", pass_config);
    run("outputs", pass_outputs);
    run("compiler_profile", pass_compiler_profile);
    run("policy", pass_policy);

    ValidationReport {
        diagnostics,
        passes_run,
    }
}

// ─── Passes ───────────────────────────────────────────────────────────────────

fn pass_schema(project: &Project, opts: &ValidateOptions, out: &mut Vec<Diagnostic>) {
    if project.id.trim().is_empty() {
        out.push(error(
            "schema",
            "project.empty_id",
            "Project id is required",
            None,
        ));
    }
    if project.name.trim().is_empty() {
        out.push(error(
            "schema",
            "project.empty_name",
            "Project name is required",
            None,
        ));
    }
    if project.schema_version.trim().is_empty() {
        out.push(error(
            "schema",
            "project.empty_schema",
            "schema_version is required",
            None,
        ));
        return;
    }
    if opts.strict_schema && project.schema_version != PROJECT_SCHEMA_VERSION {
        out.push(error(
            "schema",
            "project.unknown_schema",
            format!(
                "unsupported schema_version `{}` (expected `{PROJECT_SCHEMA_VERSION}`)",
                project.schema_version
            ),
            None,
        ));
    }
}

fn pass_unique_capabilities(project: &Project, _opts: &ValidateOptions, out: &mut Vec<Diagnostic>) {
    let mut seen = BTreeSet::new();
    for cap in &project.capabilities {
        let key = format!("{}@{}", cap.id, cap.version);
        if !seen.insert(key.clone()) {
            out.push(error(
                "unique_capabilities",
                "capability.duplicate",
                format!("Duplicate capability contract `{key}`"),
                Some(key),
            ));
        }
        if cap.id.trim().is_empty() {
            out.push(error(
                "unique_capabilities",
                "capability.empty_id",
                "Capability id is required",
                None,
            ));
        }
        if cap.version.trim().is_empty() {
            out.push(error(
                "unique_capabilities",
                "capability.empty_version",
                format!("Capability `{}` has empty version", cap.id),
                Some(cap.id.clone()),
            ));
        }
    }
}

fn pass_unique_ports(project: &Project, _opts: &ValidateOptions, out: &mut Vec<Diagnostic>) {
    for cap in &project.capabilities {
        let key = format!("{}@{}", cap.id, cap.version);
        let mut in_ids = BTreeSet::new();
        for p in &cap.inputs {
            if p.id.trim().is_empty() {
                out.push(error(
                    "unique_ports",
                    "port.empty_id",
                    format!("Empty input port id on `{key}`"),
                    Some(key.clone()),
                ));
                continue;
            }
            if !in_ids.insert(p.id.clone()) {
                out.push(error(
                    "unique_ports",
                    "port.duplicate_input",
                    format!("Duplicate input port `{}` on `{key}`", p.id),
                    Some(format!("{key}.in.{}", p.id)),
                ));
            }
        }
        let mut out_ids = BTreeSet::new();
        for p in &cap.outputs {
            if p.id.trim().is_empty() {
                out.push(error(
                    "unique_ports",
                    "port.empty_id",
                    format!("Empty output port id on `{key}`"),
                    Some(key.clone()),
                ));
                continue;
            }
            if !out_ids.insert(p.id.clone()) {
                out.push(error(
                    "unique_ports",
                    "port.duplicate_output",
                    format!("Duplicate output port `{}` on `{key}`", p.id),
                    Some(format!("{key}.out.{}", p.id)),
                ));
            }
        }
    }
}

fn pass_unique_instances(project: &Project, _opts: &ValidateOptions, out: &mut Vec<Diagnostic>) {
    let mut seen_ids = BTreeSet::new();
    for instance in &project.instances {
        if instance.id.trim().is_empty() {
            out.push(error(
                "unique_instances",
                "instance.empty_id",
                "Instance id is required",
                None,
            ));
            continue;
        }
        if !seen_ids.insert(instance.id.clone()) {
            out.push(error(
                "unique_instances",
                "instance.duplicate",
                format!("Duplicate instance id `{}`", instance.id),
                Some(instance.id.clone()),
            ));
        }
        if project.capability_for(&instance.capability).is_none() {
            out.push(error(
                "unique_instances",
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
}

fn pass_entrypoint(project: &Project, _opts: &ValidateOptions, out: &mut Vec<Diagnostic>) {
    if let Some(entry) = &project.entrypoint {
        if project.instance(entry).is_none() {
            out.push(error(
                "entrypoint",
                "entrypoint.missing",
                format!("Entrypoint `{entry}` does not exist"),
                Some(entry.clone()),
            ));
        }
    } else if !project.instances.is_empty() {
        out.push(warning(
            "entrypoint",
            "entrypoint.unset",
            "Project has instances but no entrypoint",
            None,
        ));
    }
}

fn pass_bindings(project: &Project, _opts: &ValidateOptions, out: &mut Vec<Diagnostic>) {
    // Type-check each binding; count targets per input for cardinality.
    let mut targets: BTreeMap<String, usize> = BTreeMap::new();
    let mut seen_edges: BTreeSet<(String, String)> = BTreeSet::new();

    for binding in &project.bindings {
        let edge_key = (binding.from.to_string(), binding.to.to_string());
        if !seen_edges.insert(edge_key) {
            out.push(error(
                "bindings",
                "binding.duplicate",
                format!("Duplicate binding {} → {}", binding.from, binding.to),
                Some(format!("{}→{}", binding.from, binding.to)),
            ));
            continue;
        }

        *targets.entry(binding.to.to_string()).or_insert(0) += 1;

        let from_ty = match port_type(project, &binding.from, true) {
            Ok(ty) => ty,
            Err(mut d) => {
                d.pass = Some("bindings".into());
                out.push(d);
                continue;
            }
        };
        let to_ty = match port_type(project, &binding.to, false) {
            Ok(ty) => ty,
            Err(mut d) => {
                d.pass = Some("bindings".into());
                out.push(d);
                continue;
            }
        };
        if !from_ty.is_compatible_with(&to_ty) {
            out.push(error(
                "bindings",
                "binding.type_mismatch",
                format!(
                    "Type mismatch: {} produces `{from_ty}`, {} expects `{to_ty}`",
                    binding.from, binding.to
                ),
                Some(format!("{}→{}", binding.from, binding.to)),
            ));
        }
    }

    // Cardinality: each input port at most one producer; required inputs exactly one.
    for instance in &project.instances {
        let Some(cap) = project.capability_for(&instance.capability) else {
            continue;
        };
        for input in &cap.inputs {
            let path = PortPath::new(&instance.id, &input.id);
            let key = path.to_string();
            let count = targets.get(&key).copied().unwrap_or(0);
            if count > 1 {
                out.push(error(
                    "bindings",
                    "binding.cardinality",
                    format!("Input `{path}` has {count} producers (at most one allowed)"),
                    Some(key.clone()),
                ));
            }
            if input.required && count == 0 {
                out.push(error(
                    "bindings",
                    "input.unbound",
                    format!("Required input `{path}` has no binding"),
                    Some(key),
                ));
            }
        }
    }
}

fn pass_cycles(project: &Project, _opts: &ValidateOptions, out: &mut Vec<Diagnostic>) {
    if project.instances.is_empty() {
        return;
    }
    if let Err(msg) = topo_order(project) {
        out.push(error("cycles", "graph.cycle", msg, None));
    }
}

fn pass_impl_compatibility(project: &Project, opts: &ValidateOptions, out: &mut Vec<Diagnostic>) {
    let by_full: BTreeMap<String, &Implementation> = opts
        .implementations
        .iter()
        .map(|i| (i.full_id(), i))
        .collect();

    for instance in &project.instances {
        let Some(impl_id) = instance.implementation.as_ref().filter(|s| !s.is_empty()) else {
            continue;
        };
        // Basic shape: id@version
        if !impl_id.contains('@') {
            out.push(warning(
                "impl_compatibility",
                "impl.missing_version",
                format!(
                    "Instance `{}` implementation `{impl_id}` is not `id@version`",
                    instance.id
                ),
                Some(instance.id.clone()),
            ));
        }

        if by_full.is_empty() {
            continue;
        }

        match by_full.get(impl_id.as_str()) {
            Some(imp) => {
                if imp.capability.id != instance.capability.id
                    || imp.capability.version != instance.capability.version
                {
                    out.push(error(
                        "impl_compatibility",
                        "impl.capability_mismatch",
                        format!(
                            "Implementation `{impl_id}` is for `{}`, instance `{}` needs `{}`",
                            imp.capability.as_key(),
                            instance.id,
                            instance.capability.as_key()
                        ),
                        Some(instance.id.clone()),
                    ));
                }
            }
            None => {
                let msg = format!(
                    "Instance `{}` selects unknown implementation `{impl_id}`",
                    instance.id
                );
                if opts.require_known_implementation {
                    out.push(error(
                        "impl_compatibility",
                        "impl.unknown",
                        msg,
                        Some(instance.id.clone()),
                    ));
                } else {
                    out.push(warning(
                        "impl_compatibility",
                        "impl.unknown",
                        msg,
                        Some(instance.id.clone()),
                    ));
                }
            }
        }
    }
}

fn pass_config(project: &Project, _opts: &ValidateOptions, out: &mut Vec<Diagnostic>) {
    for instance in &project.instances {
        let cap_key = instance.capability.as_key();
        // Pilot: path_set requires string `path`
        if cap_key == "data.json.path_set@1" {
            match instance.config.get("path") {
                None => out.push(error(
                    "config",
                    "config.missing_path",
                    format!("Instance `{}` (path_set) requires config.path", instance.id),
                    Some(instance.id.clone()),
                )),
                Some(v) if !v.is_string() => out.push(error(
                    "config",
                    "config.path_not_string",
                    format!("Instance `{}` config.path must be a string", instance.id),
                    Some(instance.id.clone()),
                )),
                Some(v) if v.as_str().map(|s| s.is_empty()).unwrap_or(true) => out.push(error(
                    "config",
                    "config.path_empty",
                    format!("Instance `{}` config.path must be non-empty", instance.id),
                    Some(instance.id.clone()),
                )),
                _ => {}
            }
            if !instance.config.contains_key("value") {
                out.push(warning(
                    "config",
                    "config.missing_value",
                    format!(
                        "Instance `{}` (path_set) has no config.value (runtime may use default)",
                        instance.id
                    ),
                    Some(instance.id.clone()),
                ));
            }
        }
    }
}

fn pass_outputs(project: &Project, _opts: &ValidateOptions, out: &mut Vec<Diagnostic>) {
    // Map: output port path → number of consumers
    let mut consumers: BTreeMap<String, usize> = BTreeMap::new();
    for b in &project.bindings {
        *consumers.entry(b.from.to_string()).or_insert(0) += 1;
    }

    for instance in &project.instances {
        let Some(cap) = project.capability_for(&instance.capability) else {
            continue;
        };
        // Sink I/O nodes with no outputs are fine.
        if cap.outputs.is_empty() {
            continue;
        }
        // Pure sources (io.input) with consumers optional — warn if completely unused.
        let mut any_used = false;
        let mut unused = Vec::new();
        for out_port in &cap.outputs {
            let path = PortPath::new(&instance.id, &out_port.id);
            let key = path.to_string();
            if consumers.get(&key).copied().unwrap_or(0) > 0 {
                any_used = true;
            } else {
                unused.push(key);
            }
        }
        // Don't warn for terminal-looking graphs where entrypoint is the only node, etc.
        // Warn when a non-entrypoint-source has outputs but nothing consumes them
        // and the instance is not a declared leaf sink pattern.
        let is_io_input = cap.kind == "io" && cap.inputs.is_empty();
        if !any_used && !unused.is_empty() {
            // Always soft: unused outputs are warnings (may be intentional export points).
            for u in unused {
                out.push(warning(
                    "outputs",
                    "output.unused",
                    format!(
                        "Output `{u}` has no consumers{}",
                        if is_io_input {
                            " (source is disconnected)"
                        } else {
                            ""
                        }
                    ),
                    Some(u),
                ));
            }
        }
    }
}

fn pass_compiler_profile(project: &Project, opts: &ValidateOptions, out: &mut Vec<Diagnostic>) {
    let Some(profile) = project.metadata.get("compiler_profile") else {
        return;
    };
    if profile.trim().is_empty() {
        out.push(error(
            "compiler_profile",
            "compiler_profile.empty",
            "metadata.compiler_profile is empty",
            Some("metadata.compiler_profile".into()),
        ));
        return;
    }
    let allowed = if opts.allowed_compiler_profiles.is_empty() {
        // Default allow-list when caller cleared it via partial options.
        return;
    } else {
        &opts.allowed_compiler_profiles
    };
    if !allowed.iter().any(|a| a == profile) {
        out.push(error(
            "compiler_profile",
            "compiler_profile.unknown",
            format!(
                "unknown compiler_profile `{profile}` (allowed: {})",
                allowed.join(", ")
            ),
            Some("metadata.compiler_profile".into()),
        ));
    }
}

fn pass_policy(project: &Project, opts: &ValidateOptions, out: &mut Vec<Diagnostic>) {
    if !opts.require_release_lifecycle {
        return;
    }
    if opts.implementations.is_empty() {
        out.push(warning(
            "policy",
            "policy.no_implementations",
            "require_release_lifecycle set but no implementations provided; skip lifecycle checks",
            None,
        ));
        return;
    }
    let by_full: BTreeMap<String, &Implementation> = opts
        .implementations
        .iter()
        .map(|i| (i.full_id(), i))
        .collect();

    for instance in &project.instances {
        let Some(impl_id) = instance.implementation.as_ref().filter(|s| !s.is_empty()) else {
            // Unselected: release policy wants an explicit choice for non-IO transforms.
            if let Some(cap) = project.capability_for(&instance.capability) {
                if cap.kind != "io" {
                    out.push(error(
                        "policy",
                        "policy.impl_required",
                        format!(
                            "Release policy: instance `{}` must select an implementation",
                            instance.id
                        ),
                        Some(instance.id.clone()),
                    ));
                }
            }
            continue;
        };
        if let Some(imp) = by_full.get(impl_id.as_str()) {
            match imp.status {
                LifecycleStatus::Candidate | LifecycleStatus::InventoryOnly => {
                    out.push(error(
                        "policy",
                        "policy.candidate_forbidden",
                        format!(
                            "Release policy: `{impl_id}` is `{}` (not release-eligible)",
                            imp.status.as_str()
                        ),
                        Some(instance.id.clone()),
                    ));
                }
                LifecycleStatus::Conformant | LifecycleStatus::Admitted => {}
            }
        }
    }
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

/// Topological order of instances; Err if the graph has a cycle.
pub fn topo_order(project: &Project) -> Result<Vec<String>, String> {
    let ids: HashSet<String> = project.instances.iter().map(|i| i.id.clone()).collect();
    let mut indegree: HashMap<String, usize> = ids.iter().map(|id| (id.clone(), 0)).collect();
    let mut outgoing: HashMap<String, Vec<String>> =
        ids.iter().map(|id| (id.clone(), Vec::new())).collect();

    for binding in &project.bindings {
        let from = &binding.from.instance;
        let to = &binding.to.instance;
        if from == to {
            continue;
        }
        if !ids.contains(from) || !ids.contains(to) {
            continue;
        }
        outgoing.get_mut(from).unwrap().push(to.clone());
        *indegree.get_mut(to).unwrap() += 1;
    }

    let mut queue: VecDeque<String> = indegree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| id.clone())
        .collect();
    queue.make_contiguous().sort();

    let mut order = Vec::new();
    while let Some(id) = queue.pop_front() {
        order.push(id.clone());
        let mut nexts = outgoing.remove(&id).unwrap_or_default();
        nexts.sort();
        for n in nexts {
            let e = indegree.get_mut(&n).unwrap();
            *e -= 1;
            if *e == 0 {
                queue.push_back(n);
            }
        }
    }

    if order.len() != ids.len() {
        return Err("project graph contains a cycle".into());
    }
    Ok(order)
}

/// Resolve a port type when we know whether we expect an input or output.
pub fn port_type(
    project: &Project,
    path: &PortPath,
    as_output: bool,
) -> Result<TypeRef, Diagnostic> {
    let Some(instance) = project.instance(&path.instance) else {
        return Err(error(
            "bindings",
            "port.missing_instance",
            format!("Instance `{}` not found", path.instance),
            Some(path.to_string()),
        ));
    };
    let Some(cap) = project.capability_for(&instance.capability) else {
        return Err(error(
            "bindings",
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
                "bindings",
                "port.from_not_output",
                format!("`{path}` is an input; binding sources must be outputs"),
                Some(path.to_string()),
            ));
        }
        return Err(error(
            "bindings",
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
            "bindings",
            "port.to_not_input",
            format!("`{path}` is an output; binding targets must be inputs"),
            Some(path.to_string()),
        ));
    }
    Err(error(
        "bindings",
        "port.missing_input",
        format!("No input port `{}` on `{}`", path.port, path.instance),
        Some(path.to_string()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::{
        Binding, Capability, CapabilityRef, Implementation, ImplementationEvidence, Instance,
        PortSpec, Project, PROJECT_SCHEMA_VERSION,
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
        assert!(report.passes_run.contains(&"schema".into()));
        assert!(report.passes_run.contains(&"cycles".into()));
        assert!(report.passes_run.contains(&"policy".into()));
    }

    #[test]
    fn rejects_output_as_binding_target() {
        let mut p = pilot();
        p.bindings[0].to = PortPath::new("parse", "value");
        let report = validate_project(&p);
        assert!(!report.is_ok());
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.code == "port.to_not_input"));
    }

    #[test]
    fn rejects_unknown_schema() {
        let mut p = pilot();
        p.schema_version = "wvx.project.v9.9".into();
        let report = validate_project(&p);
        assert!(!report.is_ok());
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.code == "project.unknown_schema"));
    }

    #[test]
    fn rejects_cycle() {
        let mut p = pilot();
        // Add reverse edge: parse.value → input (type mismatch + cycle via instances)
        // Better: two transform nodes cycling
        p.capabilities.push(Capability {
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
        });
        p.instances.push(Instance {
            id: "ps".into(),
            capability: CapabilityRef::new("data.json.path_set", "1"),
            implementation: None,
            config: {
                let mut c = BTreeMap::new();
                c.insert("path".into(), serde_json::json!("/x"));
                c.insert("value".into(), serde_json::json!(1));
                c
            },
            ui: None,
        });
        p.bindings.push(Binding {
            from: PortPath::new("parse", "value"),
            to: PortPath::new("ps", "value"),
        });
        p.bindings.push(Binding {
            from: PortPath::new("ps", "value"),
            to: PortPath::new("parse", "bytes"), // wrong type too
        });
        // Force a pure cycle without type issues: parse -> ps -> parse on value ports
        // Remove bad edge and add cycle on value
        p.bindings.clear();
        p.bindings.push(Binding {
            from: PortPath::new("input", "bytes"),
            to: PortPath::new("parse", "bytes"),
        });
        // Can't cycle parse bytes with value. Use two path_set nodes.
        p.instances.push(Instance {
            id: "ps2".into(),
            capability: CapabilityRef::new("data.json.path_set", "1"),
            implementation: None,
            config: {
                let mut c = BTreeMap::new();
                c.insert("path".into(), serde_json::json!("/y"));
                c.insert("value".into(), serde_json::json!(2));
                c
            },
            ui: None,
        });
        p.bindings.push(Binding {
            from: PortPath::new("ps", "value"),
            to: PortPath::new("ps2", "value"),
        });
        p.bindings.push(Binding {
            from: PortPath::new("ps2", "value"),
            to: PortPath::new("ps", "value"),
        });
        let report = validate_project(&p);
        assert!(
            report.diagnostics.iter().any(|d| d.code == "graph.cycle"),
            "{:?}",
            report.diagnostics
        );
    }

    #[test]
    fn rejects_double_producer() {
        let mut p = pilot();
        // second producer into parse.bytes
        p.instances.push(Instance {
            id: "input2".into(),
            capability: CapabilityRef::new("io.input.bytes", "1"),
            implementation: None,
            config: Default::default(),
            ui: None,
        });
        p.bindings.push(Binding {
            from: PortPath::new("input2", "bytes"),
            to: PortPath::new("parse", "bytes"),
        });
        let report = validate_project(&p);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "binding.cardinality"),
            "{:?}",
            report.diagnostics
        );
    }

    #[test]
    fn policy_rejects_candidate() {
        let mut p = pilot();
        p.instances[1].implementation = Some("serde-json.parse-owned@1".into());
        let imp = Implementation {
            id: "serde-json.parse-owned".into(),
            version: "1".into(),
            capability: CapabilityRef::new("data.json.parse", "1"),
            source: Default::default(),
            adapter: None,
            status: LifecycleStatus::Candidate,
            evidence: ImplementationEvidence::default(),
            notes: None,
            sdk: None,
            conformance_profile: None,
            evidence_artifact: None,
            source_ref: None,
        };
        let opts = ValidateOptions {
            implementations: vec![imp],
            require_release_lifecycle: true,
            ..ValidateOptions::structural()
        };
        let report = validate_project_with(&p, &opts);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "policy.candidate_forbidden"),
            "{:?}",
            report.diagnostics
        );
    }

    #[test]
    fn path_set_requires_config_path() {
        let mut p = pilot();
        p.capabilities.push(Capability {
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
        });
        p.instances.push(Instance {
            id: "ps".into(),
            capability: CapabilityRef::new("data.json.path_set", "1"),
            implementation: None,
            config: Default::default(),
            ui: None,
        });
        p.bindings.push(Binding {
            from: PortPath::new("parse", "value"),
            to: PortPath::new("ps", "value"),
        });
        let report = validate_project(&p);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "config.missing_path"),
            "{:?}",
            report.diagnostics
        );
    }

    #[test]
    fn unknown_compiler_profile() {
        let mut p = pilot();
        p.metadata
            .insert("compiler_profile".into(), "nightly-unsafe".into());
        let report = validate_project(&p);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "compiler_profile.unknown"),
            "{:?}",
            report.diagnostics
        );
    }
}
