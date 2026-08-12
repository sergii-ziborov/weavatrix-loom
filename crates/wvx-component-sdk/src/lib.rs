//! Weavatrix Loom Component SDK — Gate F adapter ABI (ADR-0011).
//!
//! External adapters register handlers at host startup without editing
//! `with_pilot()` match tables. Compiler emit uses `Implementation.sdk.emit`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use wvx_runtime::{ConfigMap, ErasedComponent, HandlerRegistry, WvxValueMap};
use wvx_types::WvxValue;

/// Global plugin table filled by adapter crates before `install_plugins`.
static PLUGINS: OnceLock<Mutex<Vec<PluginRegistration>>> = OnceLock::new();

fn plugins() -> &'static Mutex<Vec<PluginRegistration>> {
    PLUGINS.get_or_init(|| Mutex::new(Vec::new()))
}

struct PluginRegistration {
    implementation_id: String,
    capability_key: String,
    factory: Arc<dyn Fn() -> Box<dyn ErasedComponent> + Send + Sync>,
}

/// Register an SDK handler factory (call from adapter `register()`).
pub fn register_plugin<F>(
    implementation_id: impl Into<String>,
    capability_key: impl Into<String>,
    factory: F,
) where
    F: Fn() -> Box<dyn ErasedComponent> + Send + Sync + 'static,
{
    plugins()
        .lock()
        .expect("plugin lock")
        .push(PluginRegistration {
            implementation_id: implementation_id.into(),
            capability_key: capability_key.into(),
            factory: Arc::new(factory),
        });
}

/// Install all registered plugins into a handler registry.
///
/// Safe to call after every adapter crate's `register()`; does not clear pilot handlers.
pub fn install_plugins(reg: &mut HandlerRegistry) {
    let list = plugins().lock().expect("plugin lock");
    for p in list.iter() {
        let handler = (p.factory)();
        // Prefer explicit default only if capability has no default yet.
        reg.register(SdkComponentBox {
            implementation_id: p.implementation_id.clone(),
            capability_key: p.capability_key.clone(),
            inner: handler,
        });
    }
}

/// Thin wrapper so boxed dyn handlers can be registered with owned ids.
struct SdkComponentBox {
    implementation_id: String,
    capability_key: String,
    inner: Box<dyn ErasedComponent>,
}

impl ErasedComponent for SdkComponentBox {
    fn implementation_id(&self) -> &str {
        &self.implementation_id
    }
    fn capability_key(&self) -> &str {
        &self.capability_key
    }
    fn execute(&self, inputs: &WvxValueMap, config: &ConfigMap) -> Result<WvxValueMap, String> {
        self.inner.execute(inputs, config)
    }
}

/// Helper: wrap `fn(&[u8]) -> Result<Value, String>` as parse-style handler.
pub fn bytes_to_json_handler(
    implementation_id: impl Into<String>,
    capability_key: impl Into<String>,
    f: fn(&[u8]) -> Result<serde_json::Value, String>,
) -> Box<dyn ErasedComponent> {
    Box::new(BytesToJson {
        implementation_id: implementation_id.into(),
        capability_key: capability_key.into(),
        f,
    })
}

struct BytesToJson {
    implementation_id: String,
    capability_key: String,
    f: fn(&[u8]) -> Result<serde_json::Value, String>,
}

impl ErasedComponent for BytesToJson {
    fn implementation_id(&self) -> &str {
        &self.implementation_id
    }
    fn capability_key(&self) -> &str {
        &self.capability_key
    }
    fn execute(&self, inputs: &WvxValueMap, _config: &ConfigMap) -> Result<WvxValueMap, String> {
        let bytes = match inputs.get("bytes") {
            Some(WvxValue::Bytes(b)) => b.as_slice(),
            Some(_) => return Err("data.json.parse: port `bytes` must be bytes".into()),
            None => return Err("data.json.parse: missing port `bytes`".into()),
        };
        let value = (self.f)(bytes)?;
        let mut out = WvxValueMap::new();
        out.insert("value".into(), WvxValue::Json(value));
        Ok(out)
    }
}

/// Helper: wrap `fn(&Value) -> Result<Vec<u8>, String>` as serialize-style handler.
pub fn json_to_bytes_handler(
    implementation_id: impl Into<String>,
    capability_key: impl Into<String>,
    f: fn(&serde_json::Value) -> Result<Vec<u8>, String>,
) -> Box<dyn ErasedComponent> {
    Box::new(JsonToBytes {
        implementation_id: implementation_id.into(),
        capability_key: capability_key.into(),
        f,
    })
}

struct JsonToBytes {
    implementation_id: String,
    capability_key: String,
    f: fn(&serde_json::Value) -> Result<Vec<u8>, String>,
}

impl ErasedComponent for JsonToBytes {
    fn implementation_id(&self) -> &str {
        &self.implementation_id
    }
    fn capability_key(&self) -> &str {
        &self.capability_key
    }
    fn execute(&self, inputs: &WvxValueMap, _config: &ConfigMap) -> Result<WvxValueMap, String> {
        let value = match inputs.get("value") {
            Some(WvxValue::Json(v)) => v,
            Some(_) => return Err("data.json.serialize: port `value` must be json.value".into()),
            None => return Err("data.json.serialize: missing port `value`".into()),
        };
        let bytes = (self.f)(value)?;
        let mut out = WvxValueMap::new();
        out.insert("bytes".into(), WvxValue::Bytes(bytes));
        Ok(out)
    }
}

/// Helper: wrap `fn(Value, &str, Value) -> Result<Value, String>` as path_set handler.
pub fn path_set_handler(
    implementation_id: impl Into<String>,
    capability_key: impl Into<String>,
    f: fn(serde_json::Value, &str, serde_json::Value) -> Result<serde_json::Value, String>,
) -> Box<dyn ErasedComponent> {
    Box::new(PathSetFn {
        implementation_id: implementation_id.into(),
        capability_key: capability_key.into(),
        f,
    })
}

struct PathSetFn {
    implementation_id: String,
    capability_key: String,
    f: fn(serde_json::Value, &str, serde_json::Value) -> Result<serde_json::Value, String>,
}

impl ErasedComponent for PathSetFn {
    fn implementation_id(&self) -> &str {
        &self.implementation_id
    }
    fn capability_key(&self) -> &str {
        &self.capability_key
    }
    fn execute(&self, inputs: &WvxValueMap, config: &ConfigMap) -> Result<WvxValueMap, String> {
        let value = match inputs.get("value") {
            Some(WvxValue::Json(v)) => v.clone(),
            Some(_) => return Err("data.json.path_set: port `value` must be json.value".into()),
            None => return Err("data.json.path_set: missing port `value`".into()),
        };
        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "data.json.path_set: config.path (string) is required".to_string())?;
        let set_to = config
            .get("value")
            .cloned()
            .ok_or_else(|| "data.json.path_set: config.value is required".to_string())?;
        let out_v = (self.f)(value, path, set_to)?;
        let mut out = WvxValueMap::new();
        out.insert("value".into(), WvxValue::Json(out_v));
        Ok(out)
    }
}

/// Wire descriptor mirrored from registry `Implementation.sdk` (for tooling).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkEmitSpec {
    pub crate_name: String,
    #[serde(default)]
    pub crate_path: Option<String>,
    pub template: String,
}

/// Substitute `{port}` placeholders in an emit template.
pub fn render_emit_template(
    template: &str,
    input_exprs: &BTreeMap<String, String>,
) -> Result<String, String> {
    let mut out = template.to_string();
    for (port, expr) in input_exprs {
        let needle = format!("{{{port}}}");
        if !out.contains(&needle) {
            continue;
        }
        out = out.replace(&needle, expr);
    }
    if out.contains('{') && out.contains('}') {
        // leftover placeholders
        return Err(format!(
            "sdk emit template has unresolved placeholders: {out}"
        ));
    }
    Ok(out)
}

/// Build a full HandlerRegistry: pilot handlers + SDK plugins.
pub fn registry_with_pilot_and_plugins() -> HandlerRegistry {
    let mut reg = HandlerRegistry::with_pilot();
    install_plugins(&mut reg);
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_render() {
        let mut m = BTreeMap::new();
        m.insert("bytes".into(), "input_bytes".into());
        let s = render_emit_template(
            "demo::parse({bytes}.as_slice())?",
            &m,
        )
        .unwrap();
        assert_eq!(s, "demo::parse(input_bytes.as_slice())?");
    }
}
