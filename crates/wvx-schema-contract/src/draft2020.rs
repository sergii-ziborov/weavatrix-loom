//! Focused JSON Schema Draft 2020-12 evaluator for Loom wire documents.
//!
//! Covers the keywords our schemas actually use: `type`, `const`, `required`,
//! `properties`, `additionalProperties`, `items`, `enum`, `minLength`,
//! `minimum`, `$ref` (same-document `#/$defs/...` only).

use crate::ContractError;
use serde_json::Value;
use std::collections::BTreeMap;

pub fn validate_instance(
    schema: &Value,
    instance: &Value,
    label: &str,
) -> Result<(), ContractError> {
    let defs = schema.get("$defs").cloned().unwrap_or(Value::Null);
    eval(schema, schema, instance, &defs, label)
}

fn eval(
    root: &Value,
    schema: &Value,
    instance: &Value,
    defs: &Value,
    path: &str,
) -> Result<(), ContractError> {
    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        let resolved = resolve_ref(root, r, defs)?;
        return eval(root, resolved, instance, defs, path);
    }
    if let Some(c) = schema.get("const") {
        if instance != c {
            return err(format!("{path}: const mismatch"));
        }
    }
    if let Some(en) = schema.get("enum").and_then(|v| v.as_array()) {
        if !en.iter().any(|e| e == instance) {
            return err(format!("{path}: value not in enum"));
        }
    }
    if let Some(ty) = schema.get("type") {
        check_type(ty, instance, path)?;
    }
    if let Some(min) = schema.get("minLength").and_then(|v| v.as_u64()) {
        if let Some(s) = instance.as_str() {
            if (s.chars().count() as u64) < min {
                return err(format!("{path}: minLength {min}"));
            }
        }
    }
    if let Some(min) = schema.get("minimum").and_then(|v| v.as_i64()) {
        if let Some(n) = instance.as_i64() {
            if n < min {
                return err(format!("{path}: minimum {min}"));
            }
        }
    }
    if let Some(obj) = instance.as_object() {
        if let Some(req) = schema.get("required").and_then(|v| v.as_array()) {
            for k in req {
                let key = k.as_str().unwrap_or("");
                if !obj.contains_key(key) {
                    return err(format!("{path}: missing required `{key}`"));
                }
            }
        }
        let props: BTreeMap<&str, &Value> = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .map(|o| o.iter().map(|(k, v)| (k.as_str(), v)).collect())
            .unwrap_or_default();
        let additional = schema.get("additionalProperties");
        for (k, v) in obj {
            if let Some(sub) = props.get(k.as_str()) {
                eval(root, sub, v, defs, &format!("{path}.{k}"))?;
            } else if let Some(add) = additional {
                if add.as_bool() == Some(false) {
                    return err(format!("{path}: additional property `{k}` not allowed"));
                }
                if add.is_object() {
                    eval(root, add, v, defs, &format!("{path}.{k}"))?;
                }
            }
        }
    }
    if let Some(arr) = instance.as_array() {
        if let Some(items) = schema.get("items") {
            for (i, item) in arr.iter().enumerate() {
                eval(root, items, item, defs, &format!("{path}[{i}]"))?;
            }
        }
    }
    Ok(())
}

fn check_type(ty: &Value, instance: &Value, path: &str) -> Result<(), ContractError> {
    let ok = match ty.as_str() {
        Some("object") => instance.is_object(),
        Some("array") => instance.is_array(),
        Some("string") => instance.is_string(),
        Some("integer") => instance.as_i64().is_some() || instance.as_u64().is_some(),
        Some("number") => instance.is_number(),
        Some("boolean") => instance.is_boolean(),
        Some("null") => instance.is_null(),
        _ => true,
    };
    if !ok {
        return err(format!("{path}: type mismatch (expected {ty})"));
    }
    Ok(())
}

fn resolve_ref<'a>(root: &'a Value, r: &str, defs: &'a Value) -> Result<&'a Value, ContractError> {
    if let Some(name) = r.strip_prefix("#/$defs/") {
        return defs
            .get(name)
            .ok_or_else(|| ContractError::Fail(format!("unresolved $ref {r}")));
    }
    if r == "#" {
        return Ok(root);
    }
    Err(ContractError::Fail(format!("unsupported $ref {r}")))
}

fn err(msg: String) -> Result<(), ContractError> {
    Err(ContractError::Fail(msg))
}
