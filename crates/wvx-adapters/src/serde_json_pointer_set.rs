//! Adapter: serde-json.pointer-set@1
//!
//! Independent of `wvx.reference.path-set@1`: walks a JSON Pointer and assigns
//! via pointer slots (RFC 6901 unescaping), instead of a single Map::insert.
use serde_json::Value;

/// Set `set_to` at `path` (JSON Pointer, single segment in pilot v0.1).
pub fn path_set(mut value: Value, path: &str, set_to: Value) -> Result<Value, String> {
    let pointer = normalize_pointer(path)?;
    ensure_object_root(&value)?;
    // Create the target key if missing, then assign through pointer_mut.
    ensure_leaf_exists(&mut value, &pointer)?;
    match value.pointer_mut(&pointer) {
        Some(slot) => {
            *slot = set_to;
            Ok(value)
        }
        None => Err(format!(
            "data.json.path_set: failed to resolve pointer `{pointer}`"
        )),
    }
}

fn normalize_pointer(path: &str) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("data.json.path_set: empty path".into());
    }
    let pointer = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    // Pilot v0.1: only one segment (matches reference path-set contract).
    let body = pointer.trim_start_matches('/');
    if body.is_empty() || body.contains('/') {
        return Err(format!(
            "data.json.path_set: only single-segment paths supported in v0.1 (got `{path}`)"
        ));
    }
    Ok(pointer)
}

fn ensure_object_root(value: &Value) -> Result<(), String> {
    if value.is_object() {
        Ok(())
    } else {
        Err("data.json.path_set: root value must be a JSON object".into())
    }
}

/// Ensure `/key` exists so `pointer_mut` can return a slot (RFC create-on-write for leaves).
fn ensure_leaf_exists(value: &mut Value, pointer: &str) -> Result<(), String> {
    if value.pointer(pointer).is_some() {
        return Ok(());
    }
    let key = pointer
        .strip_prefix('/')
        .ok_or_else(|| format!("data.json.path_set: bad pointer `{pointer}`"))?;
    let key = unescape_token(key);
    match value {
        Value::Object(map) => {
            map.insert(key, Value::Null);
            Ok(())
        }
        _ => Err("data.json.path_set: root value must be a JSON object".into()),
    }
}

fn unescape_token(token: &str) -> String {
    // RFC 6901: ~1 → /, ~0 → ~
    token.replace("~1", "/").replace("~0", "~")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sets_new_key() {
        let v = path_set(json!({"hello": "world"}), "/tag", json!("loom")).unwrap();
        assert_eq!(v, json!({"hello": "world", "tag": "loom"}));
    }

    #[test]
    fn overwrites_existing() {
        let v = path_set(json!({"tag": "old"}), "/tag", json!("new")).unwrap();
        assert_eq!(v, json!({"tag": "new"}));
    }

    #[test]
    fn rejects_nested() {
        assert!(path_set(json!({}), "/a/b", json!(1)).is_err());
    }
}
