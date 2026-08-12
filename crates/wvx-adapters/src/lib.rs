//! External adapter crate used by Loom exports and the playground.
//!
//! Transform handlers register via `wvx-component-sdk` ([`register_pilot_plugins`]).
//! Pure functions remain the export surface for the compiler / vendoring.

pub mod json_crate_parse;
pub mod reference_json_parse;
pub mod reference_json_serialize;
pub mod reference_json_serialize_pretty;
pub mod reference_path_set;
#[cfg(feature = "host")]
pub mod register;
pub mod serde_json_parse_owned;
pub mod serde_json_pointer_set;
pub mod serde_json_serialize;

#[cfg(feature = "host")]
pub use register::register_pilot_plugins;

/// Implementation id → module path for documentation / tooling.
pub const PILOT_ADAPTERS: &[(&str, &str)] = &[
    ("serde-json.parse-owned@1", "serde_json_parse_owned"),
    ("wvx.reference.json-parse@1", "reference_json_parse"),
    ("json-crate.parse@1", "json_crate_parse"),
    ("serde-json.serialize@1", "serde_json_serialize"),
    ("wvx.reference.json-serialize@1", "reference_json_serialize"),
    (
        "wvx.reference.json-serialize-pretty@1",
        "reference_json_serialize_pretty",
    ),
    ("wvx.reference.path-set@1", "reference_path_set"),
    ("serde-json.pointer-set@1", "serde_json_pointer_set"),
];
