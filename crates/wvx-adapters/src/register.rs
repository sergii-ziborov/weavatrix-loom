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
    base64_standard_decode, base64_standard_encode, blake3_hash, flate2_gunzip,
    flate2_gunzip_chunked, flate2_gunzip_take, flate2_gzip, flate2_gzip_chunked,
    flate2_gzip_oneshot, json_crate_parse, reference_base64_decode, reference_base64_encode,
    reference_hex_decode, reference_hex_decode_table, reference_hex_encode,
    reference_hex_encode_chunked, reference_json_parse, reference_json_serialize,
    reference_json_serialize_pretty, reference_path_set, reference_text_ascii_lower,
    reference_text_ascii_upper, reference_text_lowercase, reference_text_uppercase,
    serde_json_parse_owned, serde_json_pointer_set, serde_json_serialize, sha2_sha256,
    sha2_sha256_chunked, sha2_sha256_streaming, sha2_sha256_update_all,
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
        register_plugin(
            "wvx.reference.json-serialize@1",
            "data.json.serialize@1",
            || {
                json_to_bytes_handler(
                    "wvx.reference.json-serialize@1",
                    "data.json.serialize@1",
                    reference_json_serialize::serialize,
                )
            },
        );
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

        // Unicode text (separate from ASCII byte transforms — Milestone 1)
        register_plugin(
            "wvx.reference.text-uppercase@1",
            "data.text.unicode_uppercase@1",
            || {
                bytes_to_bytes_handler(
                    "wvx.reference.text-uppercase@1",
                    "data.text.unicode_uppercase@1",
                    reference_text_uppercase::transform,
                )
            },
        );
        register_plugin(
            "wvx.reference.text-lowercase@1",
            "data.text.unicode_lowercase@1",
            || {
                bytes_to_bytes_handler(
                    "wvx.reference.text-lowercase@1",
                    "data.text.unicode_lowercase@1",
                    reference_text_lowercase::transform,
                )
            },
        );
        // ASCII-only capabilities (must not claim Unicode)
        register_plugin(
            "wvx.reference.text-ascii-upper@1",
            "data.text.ascii_uppercase@1",
            || {
                bytes_to_bytes_handler(
                    "wvx.reference.text-ascii-upper@1",
                    "data.text.ascii_uppercase@1",
                    reference_text_ascii_upper::transform,
                )
            },
        );
        register_plugin(
            "wvx.reference.text-ascii-lower@1",
            "data.text.ascii_lowercase@1",
            || {
                bytes_to_bytes_handler(
                    "wvx.reference.text-ascii-lower@1",
                    "data.text.ascii_lowercase@1",
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
        register_plugin("sha2.sha256-chunked@1", "data.hash.sha256@1", || {
            bytes_to_named_bytes_handler(
                "sha2.sha256-chunked@1",
                "data.hash.sha256@1",
                "digest",
                sha2_sha256_chunked::digest,
            )
        });
        register_plugin("sha2.sha256-update-all@1", "data.hash.sha256@1", || {
            bytes_to_named_bytes_handler(
                "sha2.sha256-update-all@1",
                "data.hash.sha256@1",
                "digest",
                sha2_sha256_update_all::digest,
            )
        });

        // Domain 3 — compression (gzip / gunzip); multi-impl paths
        register_plugin("flate2.gzip@1", "data.compress.gzip@1", || {
            bytes_to_bytes_handler(
                "flate2.gzip@1",
                "data.compress.gzip@1",
                flate2_gzip::compress,
            )
        });
        register_plugin("flate2.gzip-chunked@1", "data.compress.gzip@1", || {
            bytes_to_bytes_handler(
                "flate2.gzip-chunked@1",
                "data.compress.gzip@1",
                flate2_gzip_chunked::compress,
            )
        });
        register_plugin("flate2.gzip-oneshot@1", "data.compress.gzip@1", || {
            bytes_to_bytes_handler(
                "flate2.gzip-oneshot@1",
                "data.compress.gzip@1",
                flate2_gzip_oneshot::compress,
            )
        });
        register_plugin("flate2.gunzip@1", "data.compress.gunzip@1", || {
            bytes_to_bytes_handler(
                "flate2.gunzip@1",
                "data.compress.gunzip@1",
                flate2_gunzip::decompress,
            )
        });
        register_plugin("flate2.gunzip-chunked@1", "data.compress.gunzip@1", || {
            bytes_to_bytes_handler(
                "flate2.gunzip-chunked@1",
                "data.compress.gunzip@1",
                flate2_gunzip_chunked::decompress,
            )
        });
        register_plugin("flate2.gunzip-take@1", "data.compress.gunzip@1", || {
            bytes_to_bytes_handler(
                "flate2.gunzip-take@1",
                "data.compress.gunzip@1",
                flate2_gunzip_take::decompress,
            )
        });

        // Domain 4 — binary codecs (hex + base64); multi-impl equality
        register_plugin(
            "wvx.reference.hex-encode@1",
            "data.codec.hex_encode@1",
            || {
                bytes_to_bytes_handler(
                    "wvx.reference.hex-encode@1",
                    "data.codec.hex_encode@1",
                    reference_hex_encode::encode,
                )
            },
        );
        register_plugin(
            "wvx.reference.hex-encode-chunked@1",
            "data.codec.hex_encode@1",
            || {
                bytes_to_bytes_handler(
                    "wvx.reference.hex-encode-chunked@1",
                    "data.codec.hex_encode@1",
                    reference_hex_encode_chunked::encode,
                )
            },
        );
        register_plugin(
            "wvx.reference.hex-decode@1",
            "data.codec.hex_decode@1",
            || {
                bytes_to_bytes_handler(
                    "wvx.reference.hex-decode@1",
                    "data.codec.hex_decode@1",
                    reference_hex_decode::decode,
                )
            },
        );
        register_plugin(
            "wvx.reference.hex-decode-table@1",
            "data.codec.hex_decode@1",
            || {
                bytes_to_bytes_handler(
                    "wvx.reference.hex-decode-table@1",
                    "data.codec.hex_decode@1",
                    reference_hex_decode_table::decode,
                )
            },
        );
        register_plugin(
            "base64.standard-encode@1",
            "data.codec.base64_encode@1",
            || {
                bytes_to_bytes_handler(
                    "base64.standard-encode@1",
                    "data.codec.base64_encode@1",
                    base64_standard_encode::encode,
                )
            },
        );
        register_plugin(
            "wvx.reference.base64-encode@1",
            "data.codec.base64_encode@1",
            || {
                bytes_to_bytes_handler(
                    "wvx.reference.base64-encode@1",
                    "data.codec.base64_encode@1",
                    reference_base64_encode::encode,
                )
            },
        );
        register_plugin(
            "base64.standard-decode@1",
            "data.codec.base64_decode@1",
            || {
                bytes_to_bytes_handler(
                    "base64.standard-decode@1",
                    "data.codec.base64_decode@1",
                    base64_standard_decode::decode,
                )
            },
        );
        register_plugin(
            "wvx.reference.base64-decode@1",
            "data.codec.base64_decode@1",
            || {
                bytes_to_bytes_handler(
                    "wvx.reference.base64-decode@1",
                    "data.codec.base64_decode@1",
                    reference_base64_decode::decode,
                )
            },
        );
    });
}
