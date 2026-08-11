//! External adapter crate used by Loom exports and the playground pilot.
//!
//! Each module maps to one `implementation_id` in the registry.

pub mod reference_json_parse;
pub mod reference_json_serialize;
pub mod reference_json_serialize_pretty;
pub mod reference_path_set;
pub mod serde_json_parse_owned;
pub mod serde_json_serialize;

/// Implementation id → module path for documentation / tooling.
pub const PILOT_ADAPTERS: &[(&str, &str)] = &[
    ("serde-json.parse-owned@1", "serde_json_parse_owned"),
    ("wvx.reference.json-parse@1", "reference_json_parse"),
    ("serde-json.serialize@1", "serde_json_serialize"),
    ("wvx.reference.json-serialize@1", "reference_json_serialize"),
    (
        "wvx.reference.json-serialize-pretty@1",
        "reference_json_serialize_pretty",
    ),
    ("wvx.reference.path-set@1", "reference_path_set"),
];
