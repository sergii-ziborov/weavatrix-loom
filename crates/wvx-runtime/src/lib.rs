//! Dynamic playground: execute a validated WVX graph with erased values.
//!
//! Production performance is measured on compiled native adapters, not here.

use std::collections::BTreeMap;
use thiserror::Error;
use wvx_ir::{PortPath, Project};
use wvx_types::WvxValue;
use wvx_validator::validate_project;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("project is not valid: {0}")]
    InvalidProject(String),
    #[error("no handler registered for capability `{0}`")]
    MissingHandler(String),
    #[error("missing input `{0}`")]
    MissingInput(String),
    #[error("component `{0}`: {1}")]
    Component(String, String),
}

pub type WvxValueMap = BTreeMap<String, WvxValue>;

/// Erased component: inputs by port id → outputs by port id.
pub trait ErasedComponent: Send + Sync {
    fn capability_key(&self) -> &str;
    fn execute(&self, inputs: &WvxValueMap) -> Result<WvxValueMap, String>;
}

#[derive(Debug, Clone)]
pub struct NodeTrace {
    pub instance_id: String,
    pub capability: String,
    pub duration_ms: f64,
    pub outputs: WvxValueMap,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub traces: Vec<NodeTrace>,
    pub outputs: WvxValueMap,
}

/// Registry of playground handlers keyed by `capability@id@version` (see CapabilityRef::as_key).
#[derive(Default)]
pub struct HandlerRegistry {
    handlers: BTreeMap<String, Box<dyn ErasedComponent>>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, handler: impl ErasedComponent + 'static) {
        self.handlers
            .insert(handler.capability_key().to_string(), Box::new(handler));
    }

    pub fn get(&self, key: &str) -> Option<&dyn ErasedComponent> {
        self.handlers.get(key).map(|h| h.as_ref())
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

    // Seed entrypoint outputs if provided as `instance.port` or bare port names on entrypoint.
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
        let Some(handler) = handlers.get(&cap_key) else {
            // I/O sources may already have outputs seeded.
            let cap = project.capability_for(&instance.capability);
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
            return Err(RuntimeError::MissingHandler(cap_key));
        };

        let mut inputs = WvxValueMap::new();
        if let Some(cap) = project.capability_for(&instance.capability) {
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
        match handler.execute(&inputs) {
            Ok(outputs) => {
                for (port, value) in &outputs {
                    port_values.insert(format!("{instance_id}.{port}"), value.clone());
                }
                traces.push(NodeTrace {
                    instance_id: instance_id.clone(),
                    capability: cap_key,
                    duration_ms: started.elapsed().as_secs_f64() * 1000.0,
                    outputs,
                    error: None,
                });
            }
            Err(err) => {
                traces.push(NodeTrace {
                    instance_id: instance_id.clone(),
                    capability: cap_key.clone(),
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
        fn capability_key(&self) -> &str {
            "test.identity.bytes@1"
        }
        fn execute(&self, inputs: &WvxValueMap) -> Result<WvxValueMap, String> {
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
            implementation: None,
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
}
