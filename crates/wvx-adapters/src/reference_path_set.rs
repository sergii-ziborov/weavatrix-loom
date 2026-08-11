//! Adapter: wvx.reference.path-set@1
use serde_json::Value;

pub fn path_set(mut value: Value, path: &str, set_to: Value) -> Result<Value, String> {
    let key = path.trim().trim_start_matches('/');
    if key.is_empty() || key.contains('/') {
        return Err(format!(
            "data.json.path_set: only single-segment paths supported in v0.1 (got `{path}`)"
        ));
    }
    match &mut value {
        Value::Object(map) => {
            map.insert(key.to_string(), set_to);
            Ok(value)
        }
        _ => Err("data.json.path_set: root value must be a JSON object".into()),
    }
}
