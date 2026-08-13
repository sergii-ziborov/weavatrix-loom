//! Host-side SDK plugin registration for all pilot transform adapters.
//!
//! Runtime `with_pilot()` only keeps I/O; transforms come from this table
//! (full SDK path — no handler structs in wvx-runtime).

#![cfg(feature = "host")]

use std::sync::Once;

use wvx_component_sdk::{
    bytes_to_bytes_handler, bytes_to_json_handler, bytes_to_named_bytes_handler,
    json_to_bytes_handler, path_set_handler, register_plugin,
};

use crate::{
    blake3_hash, json_crate_parse, reference_json_parse, reference_json_serialize,
    reference_json_serialize_pretty, reference_path_set, reference_text_ascii_lower,
    reference_text_ascii_upper, reference_text_lowercase, reference_text_uppercase,
    serde_json_parse_owned, serde_json_pointer_set, serde_json_serialize, sha2_sha256,
    sha2_sha256_streaming,
};

/// Register all pilot transform implementations into the SDK plugin table.
///
/// Safe to call multiple times (Once). Order sets first-seen defaults per capability.
pub fn register_pilot_plugins() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // parse — default first
        register_plugin("serde-json.parse-owned@1", "data.json.parse@1", || {
            bytes_to_json_handler(
                "serde-json.parse-owned@1",
                "data.json.parse@1",
                serde_json_parse_owned::parse,
            )
        });
        register_plugin("wvx.reference.json-parse@1", "data.json.parse@1", || {
            bytes_to_json_handler(
                "wvx.reference.json-parse@1",
                "data.json.parse@1",
                reference_json_parse::parse,
            )
        });
        register_plugin("json-crate.parse@1", "data.json.parse@1", || {
            bytes_to_json_handler(
                "json-crate.parse@1",
                "data.json.parse@1",
                json_crate_parse::parse,
            )
        });

        // serialize — default first
        register_plugin("serde-json.serialize@1", "data.json.serialize@1", || {
            json_to_bytes_handler(
                "serde-json.serialize@1",
                "data.json.serialize@1",
                serde_json_serialize::serialize,
            )
        });
        register_plugin("wvx.reference.json-serialize@1", "data.json.serialize@1", || {
            json_to_bytes_handler(
                "wvx.reference.json-serialize@1",
                "data.json.serialize@1",
                reference_json_serialize::serialize,
            )
        });
        register_plugin(
            "wvx.reference.json-serialize-pretty@1",
            "data.json.serialize@1",
            || {
                json_to_bytes_handler(
                    "wvx.reference.json-serialize-pretty@1",
                    "data.json.serialize@1",
                    reference_json_serialize_pretty::serialize,
                )
            },
        );

        // path_set — default first
        register_plugin("wvx.reference.path-set@1", "data.json.path_set@1", || {
            path_set_handler(
                "wvx.reference.path-set@1",
                "data.json.path_set@1",
                reference_path_set::path_set,
            )
        });
        register_plugin("serde-json.pointer-set@1", "data.json.path_set@1", || {
            path_set_handler(
                "serde-json.pointer-set@1",
                "data.json.path_set@1",
                serde_json_pointer_set::path_set,
            )
        });

        // data.text.* — second capability family (bytes → bytes)
        register_plugin(
            "wvx.reference.text-uppercase@1",
            "data.text.uppercase@1",
            || {
                bytes_to_bytes_handler(
                    "wvx.reference.text-uppercase@1",
                    "data.text.uppercase@1",
                    reference_text_uppercase::transform,
                )
            },
        );
        register_plugin(
            "wvx.reference.text-ascii-upper@1",
            "data.text.uppercase@1",
            || {
                bytes_to_bytes_handler(
                    "wvx.reference.text-ascii-upper@1",
                    "data.text.uppercase@1",
                    reference_text_ascii_upper::transform,
                )
            },
        );
        register_plugin(
            "wvx.reference.text-lowercase@1",
            "data.text.lowercase@1",
            || {
                bytes_to_bytes_handler(
                    "wvx.reference.text-lowercase@1",
                    "data.text.lowercase@1",
                    reference_text_lowercase::transform,
                )
            },
        );
        register_plugin(
            "wvx.reference.text-ascii-lower@1",
            "data.text.lowercase@1",
            || {
                bytes_to_bytes_handler(
                    "wvx.reference.text-ascii-lower@1",
                    "data.text.lowercase@1",
                    reference_text_ascii_lower::transform,
                )
            },
        );

        // Domain 2 — hashing (bytes → digest); multi-impl SHA-256 + BLAKE3
        register_plugin("sha2.sha256@1", "data.hash.sha256@1", || {
            bytes_to_named_bytes_handler(
                "sha2.sha256@1",
                "data.hash.sha256@1",
                "digest",
                sha2_sha256::digest,
            )
        });
        register_plugin("sha2.sha256-streaming@1", "data.hash.sha256@1", || {
            bytes_to_named_bytes_handler(
                "sha2.sha256-streaming@1",
                "data.hash.sha256@1",
                "digest",
                sha2_sha256_streaming::digest,
            )
        });
        register_plugin("blake3.blake3@1", "data.hash.blake3@1", || {
            bytes_to_named_bytes_handler(
                "blake3.blake3@1",
                "data.hash.blake3@1",
                "digest",
                blake3_hash::digest,
            )
        });
    });
}