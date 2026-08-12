//! Dynamic playground: execute a validated WVX graph with erased values.
//!
//! Production performance is measured on compiled native adapters, not here.

mod pilot;

pub use pilot::{list_pilot_implementations, register_pilot_handlers, PilotImplementation};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;
use wvx_ir::{PortPath, Project};
use wvx_types::WvxValue;
use wvx_validator::validate_project;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("project is not valid: {0}")]
    InvalidProject(String),
    #[error("no handler for capability `{capability}` (implementation `{implementation}`)")]
    MissingHandler {
        capability: String,
        implementation: String,
    },
    #[error("implementation `{implementation}` does not fulfill capability `{capability}`")]
    ImplementationMismatch {
        implementation: String,
        capability: String,
    },
    #[error("missing input `{0}`")]
    MissingInput(String),
    #[error("component `{0}`: {1}")]
    Component(String, String),
}

pub type WvxValueMap = BTreeMap<String, WvxValue>;
pub type ConfigMap = BTreeMap<String, serde_json::Value>;

/// Erased component: inputs by port id → outputs by port id.
pub trait ErasedComponent: Send + Sync {
    /// Stable implementation id (e.g. `serde-json.parse-owned@1`).
    fn implementation_id(&self) -> &str;
    /// Capability this implementation fulfills (`data.json.parse@1`).
    fn capability_key(&self) -> &str;
    fn execute(&self, inputs: &WvxValueMap, config: &ConfigMap) -> Result<WvxValueMap, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeTrace {
    pub instance_id: String,
    pub capability: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation: Option<String>,
    pub duration_ms: f64,
    pub outputs: WvxValueMap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub traces: Vec<NodeTrace>,
    pub outputs: WvxValueMap,
}

/// Handlers indexed by implementation id, with a default per capability.
#[derive(Default)]
pub struct HandlerRegistry {
    by_implementation: BTreeMap<String, Box<dyn ErasedComponent>>,
    /// capability_key → default implementation id
    defaults: BTreeMap<String, String>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an implementation. First registration for a capability becomes the default
    /// unless `as_default` is forced later via [`Self::set_default`].
    pub fn register(&mut self, handler: impl ErasedComponent + 'static) {
        self.register_with_default(handler, false);
    }

    pub fn register_default(&mut self, handler: impl ErasedComponent + 'static) {
        self.register_with_default(handler, true);
    }

    fn register_with_default(&mut self, handler: impl ErasedComponent + 'static, force_default: bool) {
        let impl_id = handler.implementation_id().to_string();
        let cap = handler.capability_key().to_string();
        if force_default || !self.defaults.contains_key(&cap) {
            self.defaults.insert(cap, impl_id.clone());
        }
        self.by_implementation.insert(impl_id, Box::new(handler));
    }

    pub fn set_default(&mut self, capability_key: &str, implementation_id: &str) -> bool {
        if !self.by_implementation.contains_key(implementation_id) {
            return false;
        }
        let Some(handler) = self.by_implementation.get(implementation_id) else {
            return false;
        };
        if handler.capability_key() != capability_key {
            return false;
        }
        self.defaults
            .insert(capability_key.to_string(), implementation_id.to_string());
        true
    }

    pub fn resolve(
        &self,
        capability_key: &str,
        implementation: Option<&str>,
    ) -> Result<&dyn ErasedComponent, RuntimeError> {
        let impl_id = match implementation {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => self
                .defaults
                .get(capability_key)
                .cloned()
                .unwrap_or_else(|| capability_key.to_string()),
        };

        let Some(handler) = self.by_implementation.get(&impl_id) else {
            return Err(RuntimeError::MissingHandler {
                capability: capability_key.into(),
                implementation: impl_id,
            });
        };
        if handler.capability_key() != capability_key {
            return Err(RuntimeError::ImplementationMismatch {
                implementation: impl_id,
                capability: capability_key.into(),
            });
        }
        Ok(handler.as_ref())
    }

    pub fn list_implementations(&self, capability_key: &str) -> Vec<String> {
        self.by_implementation
            .values()
            .filter(|h| h.capability_key() == capability_key)
            .map(|h| h.implementation_id().to_string())
            .collect()
    }

    pub fn with_pilot() -> Self {
        let mut reg = Self::new();
        register_pilot_handlers(&mut reg);
        reg
    }
}

/// Apply instance implementation overrides: `instance_id → implementation_id`.
pub fn apply_implementation_overrides(
    project: &mut Project,
    overrides: &BTreeMap<String, String>,
) {
    for instance in &mut project.instances {
        if let Some(impl_id) = overrides.get(&instance.id) {
            instance.implementation = Some(impl_id.clone());
        }
    }
}

/// Execute instances in binding order (simple Kahn-style topo for v0.1 DAGs).
pub fn run_project(
    project: &Project,
    handlers: &HandlerRegistry,
    entry_outputs: WvxValueMap,
) -> Result<RunResult, RuntimeError> {
    let report = validate_project(project);
    if !report.is_ok() {
        let msg = report
            .errors()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(RuntimeError::InvalidProject(msg));
    }

    let order = topo_order(project).map_err(RuntimeError::InvalidProject)?;
    let mut port_values: BTreeMap<String, WvxValue> = BTreeMap::new();

    for (k, v) in entry_outputs {
        if k.contains('.') {
            port_values.insert(k, v);
        } else if let Some(entry) = &project.entrypoint {
            port_values.insert(format!("{entry}.{k}"), v);
        } else {
            port_values.insert(k, v);
        }
    }

    let mut traces = Vec::new();

    for instance_id in order {
        let instance = project
            .instance(&instance_id)
            .expect("topo order only yields known instances");
        let cap_key = instance.capability.as_key();
        let cap = project.capability_for(&instance.capability);

        let fully_seeded = cap
            .map(|c| {
                !c.outputs.is_empty()
                    && c.inputs.is_empty()
                    && c.outputs.iter().all(|o| {
                        port_values.contains_key(&format!("{instance_id}.{}", o.id))
                    })
            })
            .unwrap_or(false);
        if fully_seeded {
            traces.push(NodeTrace {
                instance_id: instance_id.clone(),
                capability: cap_key.clone(),
                implementation: instance.implementation.clone(),
                duration_ms: 0.0,
                outputs: cap
                    .map(|c| {
                        c.outputs
                            .iter()
                            .filter_map(|o| {
                                let key = format!("{instance_id}.{}", o.id);
                                port_values
                                    .get(&key)
                                    .map(|v| (o.id.clone(), v.clone()))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                error: None,
            });
            continue;
        }

        let handler = match handlers.resolve(&cap_key, instance.implementation.as_deref()) {
            Ok(h) => h,
            Err(err) => {
                // Allow missing handler only if outputs already partially seeded.
                let has_seed = cap
                    .map(|c| {
                        c.outputs.iter().any(|o| {
                            port_values.contains_key(&format!("{instance_id}.{}", o.id))
                        })
                    })
                    .unwrap_or(false);
                if has_seed {
                    continue;
                }
                return Err(err);
            }
        };

        let chosen_impl = handler.implementation_id().to_string();

        let mut inputs = WvxValueMap::new();
        if let Some(cap) = cap {
            for input in &cap.inputs {
                let path = PortPath::new(&instance_id, &input.id);
                let value = project
                    .bindings
                    .iter()
                    .find(|b| b.to == path)
                    .and_then(|b| port_values.get(&b.from.to_string()))
                    .cloned();
                match value {
                    Some(v) => {
                        inputs.insert(input.id.clone(), v);
                    }
                    None if input.required => {
                        return Err(RuntimeError::MissingInput(path.to_string()));
                    }
                    None => {}
                }
            }
        }

        let started = std::time::Instant::now();
        match handler.execute(&inputs, &instance.config) {
            Ok(outputs) => {
                for (port, value) in &outputs {
                    port_values.insert(format!("{instance_id}.{port}"), value.clone());
                }
                traces.push(NodeTrace {
                    instance_id: instance_id.clone(),
                    capability: cap_key,
                    implementation: Some(chosen_impl),
                    duration_ms: started.elapsed().as_secs_f64() * 1000.0,
                    outputs,
                    error: None,
                });
            }
            Err(err) => {
                traces.push(NodeTrace {
                    instance_id: instance_id.clone(),
                    capability: cap_key,
                    implementation: Some(chosen_impl),
                    duration_ms: started.elapsed().as_secs_f64() * 1000.0,
                    outputs: WvxValueMap::new(),
                    error: Some(err.clone()),
                });
                return Err(RuntimeError::Component(instance_id, err));
            }
        }
    }

    Ok(RunResult {
        traces,
        outputs: port_values,
    })
}

fn topo_order(project: &Project) -> Result<Vec<String>, String> {
    use std::collections::{HashMap, HashSet, VecDeque};

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

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::{
        Binding, Capability, CapabilityRef, Instance, PortSpec, Project, PROJECT_SCHEMA_VERSION,
    };
    use wvx_types::{TypeRef, WvxValue};

    struct IdentityBytes;

    impl ErasedComponent for IdentityBytes {
        fn implementation_id(&self) -> &str {
            "test.identity.bytes@1"
        }
        fn capability_key(&self) -> &str {
            "test.identity.bytes@1"
        }
        fn execute(&self, inputs: &WvxValueMap, _config: &ConfigMap) -> Result<WvxValueMap, String> {
            let mut out = WvxValueMap::new();
            out.insert(
                "bytes".into(),
                inputs.get("bytes").cloned().unwrap_or(WvxValue::Bytes(vec![])),
            );
            Ok(out)
        }
    }

    #[test]
    fn runs_single_transform() {
        let mut project = Project::new("t", "t");
        project.schema_version = PROJECT_SCHEMA_VERSION.into();
        project.capabilities.push(Capability {
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
        project.capabilities.push(Capability {
            id: "test.identity.bytes".into(),
            version: "1".into(),
            kind: "transform".into(),
            inputs: vec![PortSpec {
                id: "bytes".into(),
                ty: TypeRef::Bytes,
                required: true,
            }],
            outputs: vec![PortSpec {
                id: "bytes".into(),
                ty: TypeRef::Bytes,
                required: true,
            }],
            errors: vec![],
            effects: vec![],
        });
        project.instances.push(Instance {
            id: "in".into(),
            capability: CapabilityRef::new("io.input.bytes", "1"),
            implementation: None,
            config: Default::default(),
            ui: None,
        });
        project.instances.push(Instance {
            id: "id".into(),
            capability: CapabilityRef::new("test.identity.bytes", "1"),
            implementation: Some("test.identity.bytes@1".into()),
            config: Default::default(),
            ui: None,
        });
        project.bindings.push(Binding {
            from: PortPath::new("in", "bytes"),
            to: PortPath::new("id", "bytes"),
        });
        project.entrypoint = Some("in".into());

        let mut handlers = HandlerRegistry::new();
        handlers.register(IdentityBytes);

        let mut seed = WvxValueMap::new();
        seed.insert("bytes".into(), WvxValue::Bytes(b"hi".to_vec()));

        let result = run_project(&project, &handlers, seed).unwrap();
        assert_eq!(
            result.outputs.get("id.bytes"),
            Some(&WvxValue::Bytes(b"hi".to_vec()))
        );
    }

    // Pilot transform swap / pretty-serialize coverage lives in wvx-conformance
    // (SDK plugins) so wvx-runtime stays free of adapters circular deps.
}
