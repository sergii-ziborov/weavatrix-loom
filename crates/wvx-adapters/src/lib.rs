//! External adapter crate used by Loom exports and the playground.
//!
//! Transform handlers register via `wvx-component-sdk` ([`register_pilot_plugins`]).
//! Pure functions remain the export surface for the compiler / vendoring.

pub mod base64_standard_decode;
pub mod base64_standard_encode;
pub mod blake3_hash;
#[cfg(feature = "blake3-parallel")]
pub mod blake3_hash_parallel;
pub mod flate2_gunzip;
pub mod flate2_gunzip_chunked;
pub mod flate2_gunzip_take;
pub mod flate2_gzip;
pub mod flate2_gzip_chunked;
pub mod flate2_gzip_oneshot;
pub mod json_crate_parse;
pub mod reference_base64_decode;
pub mod reference_base64_encode;
pub mod reference_hex_decode;
pub mod reference_hex_decode_table;
pub mod reference_hex_encode;
pub mod reference_hex_encode_chunked;
pub mod reference_json_parse;
pub mod reference_json_serialize;
pub mod reference_json_serialize_pretty;
pub mod reference_path_set;
pub mod reference_text_ascii_lower;
pub mod reference_text_ascii_upper;
pub mod reference_text_lowercase;
pub mod reference_text_uppercase;
#[cfg(feature = "host")]
pub mod register;
pub mod serde_json_parse_owned;
pub mod serde_json_pointer_set;
pub mod serde_json_serialize;
pub mod sha2_sha256;
pub mod sha2_sha256_chunked;
pub mod sha2_sha256_streaming;
pub mod sha2_sha256_update_all;
#[cfg(feature = "simd")]
pub mod simd_json_parse;
#[cfg(feature = "sonic")]
pub mod sonic_rs_parse;
pub mod stream;
pub mod zlib_rs_gunzip;
pub mod zlib_rs_gzip;

#[cfg(feature = "host")]
pub use register::register_pilot_plugins;

/// Implementation id → module path for documentation / tooling.
pub const PILOT_ADAPTERS: &[(&str, &str)] = &[
    ("serde-json.parse-owned@1", "serde_json_parse_owned"),
    ("wvx.reference.json-parse@1", "reference_json_parse"),
    ("json-crate.parse@1", "json_crate_parse"),
    ("simd-json.parse@1", "simd_json_parse"),
    ("sonic-rs.parse@1", "sonic_rs_parse"),
    ("serde-json.serialize@1", "serde_json_serialize"),
    ("wvx.reference.json-serialize@1", "reference_json_serialize"),
    (
        "wvx.reference.json-serialize-pretty@1",
        "reference_json_serialize_pretty",
    ),
    ("wvx.reference.path-set@1", "reference_path_set"),
    ("serde-json.pointer-set@1", "serde_json_pointer_set"),
    // data.text.* family (second pilot vertical)
    ("wvx.reference.text-uppercase@1", "reference_text_uppercase"),
    (
        "wvx.reference.text-ascii-upper@1",
        "reference_text_ascii_upper",
    ),
    ("wvx.reference.text-lowercase@1", "reference_text_lowercase"),
    (
        "wvx.reference.text-ascii-lower@1",
        "reference_text_ascii_lower",
    ),
    // Domain 2 — hashing (4 multi-impl SHA-256 + blake3)
    ("sha2.sha256@1", "sha2_sha256"),
    ("sha2.sha256-streaming@1", "sha2_sha256_streaming"),
    ("sha2.sha256-chunked@1", "sha2_sha256_chunked"),
    ("sha2.sha256-update-all@1", "sha2_sha256_update_all"),
    ("blake3.blake3@1", "blake3_hash"),
    ("blake3.blake3-parallel@1", "blake3_hash_parallel"),
    // Domain 3 — compression
    ("flate2.gzip@1", "flate2_gzip"),
    ("flate2.gzip-chunked@1", "flate2_gzip_chunked"),
    ("flate2.gzip-oneshot@1", "flate2_gzip_oneshot"),
    ("flate2.gunzip@1", "flate2_gunzip"),
    ("flate2.gunzip-chunked@1", "flate2_gunzip_chunked"),
    ("flate2.gunzip-take@1", "flate2_gunzip_take"),
    ("zlib-rs.gzip@1", "zlib_rs_gzip"),
    ("zlib-rs.gunzip@1", "zlib_rs_gunzip"),
    // Domain 4 — binary codecs (hex + base64 multi-impl)
    ("wvx.reference.hex-encode@1", "reference_hex_encode"),
    (
        "wvx.reference.hex-encode-chunked@1",
        "reference_hex_encode_chunked",
    ),
    ("wvx.reference.hex-decode@1", "reference_hex_decode"),
    (
        "wvx.reference.hex-decode-table@1",
        "reference_hex_decode_table",
    ),
    ("base64.standard-encode@1", "base64_standard_encode"),
    ("wvx.reference.base64-encode@1", "reference_base64_encode"),
    ("base64.standard-decode@1", "base64_standard_decode"),
    ("wvx.reference.base64-decode@1", "reference_base64_decode"),
];
