//! Canonical boundary types for Weavatrix Loom.
//!
//! Upstream crates may use any Rust types internally. At a component boundary,
//! values are expressed in this owned, serializable type system.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Schema-level type used on ports and in contracts.
///
/// Wire form is snake_case (`json_value`). Human labels use dotted names
/// (`json.value`); both are accepted on deserialize for unit variants.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeRef {
    Unit,
    Bool,
    I64,
    U64,
    F64,
    String,
    Bytes,
    JsonValue,
    ErrorRecord,
    List(Box<TypeRef>),
    Option(Box<TypeRef>),
    /// Structural record: field name → type (ordered for stable serialization).
    Record(BTreeMap<String, TypeRef>),
    /// Tagged variant: tag → payload type.
    Variant(BTreeMap<String, TypeRef>),
    /// Closed enum of string labels.
    Enum(Vec<String>),
    /// Named type alias resolved via registry (v0.1: treated as opaque identity).
    Named(String),
}

impl<'de> Deserialize<'de> for TypeRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        parse_type_ref(&value).map_err(serde::de::Error::custom)
    }
}

fn parse_type_ref(value: &serde_json::Value) -> Result<TypeRef, String> {
    match value {
        serde_json::Value::String(s) => parse_type_ref_str(s),
        serde_json::Value::Object(map) if map.len() == 1 => {
            let (key, inner) = map.iter().next().expect("len 1");
            match key.as_str() {
                "list" => Ok(TypeRef::List(Box::new(parse_type_ref(inner)?))),
                "option" => Ok(TypeRef::Option(Box::new(parse_type_ref(inner)?))),
                "record" => {
                    let obj = inner
                        .as_object()
                        .ok_or_else(|| "record expects object".to_string())?;
                    let mut fields = BTreeMap::new();
                    for (k, v) in obj {
                        fields.insert(k.clone(), parse_type_ref(v)?);
                    }
                    Ok(TypeRef::Record(fields))
                }
                "variant" => {
                    let obj = inner
                        .as_object()
                        .ok_or_else(|| "variant expects object".to_string())?;
                    let mut cases = BTreeMap::new();
                    for (k, v) in obj {
                        cases.insert(k.clone(), parse_type_ref(v)?);
                    }
                    Ok(TypeRef::Variant(cases))
                }
                "enum" => {
                    let arr = inner
                        .as_array()
                        .ok_or_else(|| "enum expects array".to_string())?;
                    let labels = arr
                        .iter()
                        .map(|v| {
                            v.as_str()
                                .map(|s| s.to_string())
                                .ok_or_else(|| "enum labels must be strings".to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(TypeRef::Enum(labels))
                }
                "named" => {
                    let name = inner
                        .as_str()
                        .ok_or_else(|| "named expects string".to_string())?;
                    Ok(TypeRef::Named(name.into()))
                }
                other => Err(format!("unknown TypeRef variant `{other}`")),
            }
        }
        other => Err(format!("invalid TypeRef: {other}")),
    }
}

fn parse_type_ref_str(s: &str) -> Result<TypeRef, String> {
    // Canonical snake_case plus dotted display aliases used in docs/UI.
    match s {
        "unit" => Ok(TypeRef::Unit),
        "bool" => Ok(TypeRef::Bool),
        "i64" => Ok(TypeRef::I64),
        "u64" => Ok(TypeRef::U64),
        "f64" => Ok(TypeRef::F64),
        "string" => Ok(TypeRef::String),
        "bytes" => Ok(TypeRef::Bytes),
        "json_value" | "json.value" => Ok(TypeRef::JsonValue),
        "error_record" | "error.record" => Ok(TypeRef::ErrorRecord),
        other => Ok(TypeRef::Named(other.into())),
    }
}

impl TypeRef {
    pub fn list(inner: TypeRef) -> Self {
        Self::List(Box::new(inner))
    }

    pub fn option(inner: TypeRef) -> Self {
        Self::Option(Box::new(inner))
    }

    /// Structural compatibility for bindings (exact match in v0.1).
    pub fn is_compatible_with(&self, other: &TypeRef) -> bool {
        self == other
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unit => write!(f, "unit"),
            Self::Bool => write!(f, "bool"),
            Self::I64 => write!(f, "i64"),
            Self::U64 => write!(f, "u64"),
            Self::F64 => write!(f, "f64"),
            Self::String => write!(f, "string"),
            Self::Bytes => write!(f, "bytes"),
            Self::JsonValue => write!(f, "json.value"),
            Self::ErrorRecord => write!(f, "error.record"),
            Self::List(inner) => write!(f, "list<{inner}>"),
            Self::Option(inner) => write!(f, "option<{inner}>"),
            Self::Record(fields) => {
                write!(f, "record{{")?;
                let mut first = true;
                for (k, v) in fields {
                    if !first {
                        write!(f, ", ")?;
                    }
                    first = false;
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Self::Variant(cases) => {
                write!(f, "variant{{")?;
                let mut first = true;
                for (k, v) in cases {
                    if !first {
                        write!(f, ", ")?;
                    }
                    first = false;
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Self::Enum(labels) => write!(f, "enum[{}]", labels.join("|")),
            Self::Named(name) => write!(f, "{name}"),
        }
    }
}

/// Runtime value carried across erased playground boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WvxValue {
    Unit,
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    String(String),
    Bytes(Vec<u8>),
    Json(serde_json::Value),
    List(Vec<WvxValue>),
    Record(BTreeMap<String, WvxValue>),
    Variant {
        tag: String,
        value: Box<WvxValue>,
    },
    Error {
        code: String,
        message: String,
    },
}

impl WvxValue {
    pub fn type_hint(&self) -> TypeRef {
        match self {
            Self::Unit => TypeRef::Unit,
            Self::Bool(_) => TypeRef::Bool,
            Self::I64(_) => TypeRef::I64,
            Self::U64(_) => TypeRef::U64,
            Self::F64(_) => TypeRef::F64,
            Self::String(_) => TypeRef::String,
            Self::Bytes(_) => TypeRef::Bytes,
            Self::Json(_) => TypeRef::JsonValue,
            Self::List(items) => {
                let inner = items
                    .first()
                    .map(WvxValue::type_hint)
                    .unwrap_or(TypeRef::Unit);
                TypeRef::list(inner)
            }
            Self::Record(fields) => {
                let mapped = fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.type_hint()))
                    .collect();
                TypeRef::Record(mapped)
            }
            Self::Variant { tag, value } => {
                let mut cases = BTreeMap::new();
                cases.insert(tag.clone(), value.type_hint());
                TypeRef::Variant(cases)
            }
            Self::Error { .. } => TypeRef::ErrorRecord,
        }
    }
}

// serde_json is used only for the Json variant representation.
// Keep the dependency at the types crate so runtime/IR share one value model.
pub use serde_json;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_compatible_with_bytes() {
        assert!(TypeRef::Bytes.is_compatible_with(&TypeRef::Bytes));
        assert!(!TypeRef::Bytes.is_compatible_with(&TypeRef::JsonValue));
    }

    #[test]
    fn display_list() {
        assert_eq!(TypeRef::list(TypeRef::String).to_string(), "list<string>");
    }

    #[test]
    fn type_ref_accepts_dotted_aliases() {
        assert_eq!(
            serde_json::from_str::<TypeRef>(r#""json_value""#).unwrap(),
            TypeRef::JsonValue
        );
        assert_eq!(
            serde_json::from_str::<TypeRef>(r#""json.value""#).unwrap(),
            TypeRef::JsonValue
        );
        assert_eq!(
            serde_json::from_str::<TypeRef>(r#""error.record""#).unwrap(),
            TypeRef::ErrorRecord
        );
        assert_eq!(
            serde_json::from_str::<TypeRef>(r#"{"list":"string"}"#).unwrap(),
            TypeRef::list(TypeRef::String)
        );
    }
}
