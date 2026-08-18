//! Pilot micro-benchmarks for Gate E (registry trust) — **not** a formal lab fleet.
//!
//! Measures handler wall time for shared pilot vectors. Results are multi-fact
//! evidence inputs (`benchmark: pass|fail`); absolute ns are host-dependent.

use std::collections::BTreeMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use wvx_runtime::{HandlerRegistry, WvxValueMap};
use wvx_types::WvxValue;

use crate::error_code_family;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchCaseResult {
    pub capability: String,
    pub implementation: String,
    pub case: String,
    pub ok: bool,
    pub iterations: u32,
    pub warmup: u32,
    /// Mean nanoseconds per iteration (wall clock).
    pub mean_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchReport {
    pub ok: bool,
    pub cases: Vec<BenchCaseResult>,
    pub provenance: BenchProvenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchProvenance {
    pub recorded_at_unix: u64,
    pub loom_version: String,
    pub rustc: String,
    pub os: String,
    pub arch: String,
    pub input_fingerprint: String,
    pub notes: Vec<String>,
}

fn bulk_json_object(inner_bytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(inner_bytes + 16);
    out.extend_from_slice(br#"{"k":""#);
    out.resize(out.len() + inner_bytes, b'x');
    out.extend_from_slice(br#""}"#);
    out
}

/// Run pilot parse / serialize / path_set micro-benches.
pub fn run_pilot_bench(iterations: u32, warmup: u32) -> BenchReport {
    let iterations = iterations.max(1);
    let warmup = warmup.min(iterations.saturating_mul(2));
    wvx_adapters::register_pilot_plugins();
    let reg = wvx_component_sdk::registry_with_pilot_and_plugins();
    let mut cases = Vec::new();

    let parse_input = br#"{"hello":"world","n":1,"tag":"bench"}"#;
    let parse_impls = [
        "serde-json.parse-owned@1",
        "wvx.reference.json-parse@1",
        "json-crate.parse@1",
        "simd-json.parse@1",
        "sonic-rs.parse@1",
    ];
    for impl_id in parse_impls {
        cases.push(bench_parse(
            &reg,
            impl_id,
            "object_tag",
            parse_input,
            iterations,
            warmup,
        ));
    }
    let parse_bulk = bulk_json_object(64 * 1024);
    for impl_id in [
        "serde-json.parse-owned@1",
        "wvx.reference.json-parse@1",
        "simd-json.parse@1",
        "sonic-rs.parse@1",
    ] {
        cases.push(bench_parse(
            &reg,
            impl_id,
            "object_tag_bulk",
            &parse_bulk,
            iterations,
            warmup,
        ));
    }

    let sample = serde_json::json!({"hello":"world","n":1,"ok":true});
    for impl_id in ["serde-json.serialize@1", "wvx.reference.json-serialize@1"] {
        cases.push(bench_serialize(
            &reg, impl_id, "object", &sample, iterations, warmup,
        ));
    }

    for impl_id in ["wvx.reference.path-set@1", "serde-json.pointer-set@1"] {
        cases.push(bench_path_set(
            &reg,
            impl_id,
            "set_tag",
            serde_json::json!({"hello":"world"}),
            "/tag",
            serde_json::json!("loom"),
            iterations,
            warmup,
        ));
    }

    // Domain 2 — hashing (multi-impl equal digests)
    let hash_in = b"Weavatrix Loom Domain 2 bench payload";
    for impl_id in [
        "sha2.sha256@1",
        "sha2.sha256-streaming@1",
        "sha2.sha256-chunked@1",
        "sha2.sha256-update-all@1",
    ] {
        cases.push(bench_bytes_ports(
            &reg,
            "data.hash.sha256@1",
            impl_id,
            "digest_hello",
            "bytes",
            "digest",
            hash_in,
            iterations,
            warmup,
        ));
    }
    cases.push(bench_bytes_ports(
        &reg,
        "data.hash.blake3@1",
        "blake3.blake3@1",
        "digest_hello",
        "bytes",
        "digest",
        hash_in,
        iterations,
        warmup,
    ));
    let hash_bulk = vec![b'a'; 64 * 1024];
    for (cap, impl_id) in [
        ("data.hash.sha256@1", "sha2.sha256@1"),
        ("data.hash.blake3@1", "blake3.blake3@1"),
        ("data.hash.blake3@1", "blake3.blake3-parallel@1"),
    ] {
        cases.push(bench_bytes_ports(
            &reg,
            cap,
            impl_id,
            "digest_bulk",
            "bytes",
            "digest",
            &hash_bulk,
            iterations,
            warmup,
        ));
    }

    // Domain 3 — compression (gzip/gunzip multi-impl)
    let gz_in = b"Weavatrix Loom Domain 3 compression bench payload xxxxxxxxxx";
    for impl_id in [
        "flate2.gzip@1",
        "flate2.gzip-chunked@1",
        "flate2.gzip-oneshot@1",
        "zlib-rs.gzip@1",
    ] {
        cases.push(bench_bytes_ports(
            &reg,
            "data.compress.gzip@1",
            impl_id,
            "gzip_payload",
            "bytes",
            "bytes",
            gz_in,
            iterations,
            warmup,
        ));
    }
    let gz_bulk = vec![b'x'; 64 * 1024];
    cases.push(bench_bytes_ports(
        &reg,
        "data.compress.gzip@1",
        "flate2.gzip@1",
        "gzip_payload_bulk",
        "bytes",
        "bytes",
        &gz_bulk,
        iterations,
        warmup,
    ));
    cases.push(bench_bytes_ports(
        &reg,
        "data.compress.gzip@1",
        "zlib-rs.gzip@1",
        "gzip_payload_bulk",
        "bytes",
        "bytes",
        &gz_bulk,
        iterations,
        warmup,
    ));
    // Pre-compress once for gunzip benches
    let gz_bytes = match reg.resolve("data.compress.gzip@1", Some("flate2.gzip@1")) {
        Ok(h) => {
            let mut inputs = WvxValueMap::new();
            inputs.insert("bytes".into(), WvxValue::Bytes(gz_in.to_vec()));
            h.execute(&inputs, &BTreeMap::new())
                .ok()
                .and_then(|m| match m.get("bytes") {
                    Some(WvxValue::Bytes(b)) => Some(b.clone()),
                    _ => None,
                })
        }
        Err(_) => None,
    };
    if let Some(gz) = gz_bytes {
        for impl_id in [
            "flate2.gunzip@1",
            "flate2.gunzip-chunked@1",
            "flate2.gunzip-take@1",
            "zlib-rs.gunzip@1",
        ] {
            cases.push(bench_bytes_ports(
                &reg,
                "data.compress.gunzip@1",
                impl_id,
                "gunzip_payload",
                "bytes",
                "bytes",
                &gz,
                iterations,
                warmup,
            ));
        }
    }

    // Domain 4 — binary codecs (hex + base64 multi-impl)
    let codec_in = b"Weavatrix Loom Domain 4 codec bench payload";
    for impl_id in [
        "wvx.reference.hex-encode@1",
        "wvx.reference.hex-encode-chunked@1",
    ] {
        cases.push(bench_bytes_ports(
            &reg,
            "data.codec.hex_encode@1",
            impl_id,
            "hex_payload",
            "bytes",
            "bytes",
            codec_in,
            iterations,
            warmup,
        ));
    }
    let hex_enc = match reg.resolve(
        "data.codec.hex_encode@1",
        Some("wvx.reference.hex-encode@1"),
    ) {
        Ok(h) => {
            let mut inputs = WvxValueMap::new();
            inputs.insert("bytes".into(), WvxValue::Bytes(codec_in.to_vec()));
            h.execute(&inputs, &BTreeMap::new())
                .ok()
                .and_then(|m| match m.get("bytes") {
                    Some(WvxValue::Bytes(b)) => Some(b.clone()),
                    _ => None,
                })
        }
        Err(_) => None,
    };
    if let Some(hex) = &hex_enc {
        for impl_id in [
            "wvx.reference.hex-decode@1",
            "wvx.reference.hex-decode-table@1",
        ] {
            cases.push(bench_bytes_ports(
                &reg,
                "data.codec.hex_decode@1",
                impl_id,
                "hex_payload",
                "bytes",
                "bytes",
                hex,
                iterations,
                warmup,
            ));
        }
    }
    for impl_id in ["base64.standard-encode@1", "wvx.reference.base64-encode@1"] {
        cases.push(bench_bytes_ports(
            &reg,
            "data.codec.base64_encode@1",
            impl_id,
            "b64_payload",
            "bytes",
            "bytes",
            codec_in,
            iterations,
            warmup,
        ));
    }
    let b64_enc = match reg.resolve(
        "data.codec.base64_encode@1",
        Some("base64.standard-encode@1"),
    ) {
        Ok(h) => {
            let mut inputs = WvxValueMap::new();
            inputs.insert("bytes".into(), WvxValue::Bytes(codec_in.to_vec()));
            h.execute(&inputs, &BTreeMap::new())
                .ok()
                .and_then(|m| match m.get("bytes") {
                    Some(WvxValue::Bytes(b)) => Some(b.clone()),
                    _ => None,
                })
        }
        Err(_) => None,
    };
    if let Some(b64) = &b64_enc {
        for impl_id in ["base64.standard-decode@1", "wvx.reference.base64-decode@1"] {
            cases.push(bench_bytes_ports(
                &reg,
                "data.codec.base64_decode@1",
                impl_id,
                "b64_payload",
                "bytes",
                "bytes",
                b64,
                iterations,
                warmup,
            ));
        }
    }

    let ok = cases.iter().all(|c| c.ok);
    BenchReport {
        ok,
        cases,
        provenance: capture_provenance(parse_input),
    }
}

/// Generic bench for bytes → named-bytes transforms (hash digest, gzip, …).
fn bench_bytes_ports(
    reg: &HandlerRegistry,
    cap: &str,
    impl_id: &str,
    case: &str,
    in_port: &str,
    out_port: &str,
    bytes: &[u8],
    iterations: u32,
    warmup: u32,
) -> BenchCaseResult {
    let handler = match reg.resolve(cap, Some(impl_id)) {
        Ok(h) => h,
        Err(e) => {
            return fail_case(cap, impl_id, case, iterations, warmup, e.to_string());
        }
    };
    let mut inputs = WvxValueMap::new();
    inputs.insert(in_port.into(), WvxValue::Bytes(bytes.to_vec()));
    let config = BTreeMap::new();

    for _ in 0..warmup {
        match handler.execute(&inputs, &config) {
            Ok(m) => {
                if !m.contains_key(out_port) {
                    return fail_case(
                        cap,
                        impl_id,
                        case,
                        iterations,
                        warmup,
                        format!("missing output port `{out_port}`"),
                    );
                }
            }
            Err(e) => return fail_case(cap, impl_id, case, iterations, warmup, e),
        }
    }

    let mut min_ns = u64::MAX;
    let mut max_ns = 0u64;
    let mut sum_ns = 0u64;
    for _ in 0..iterations {
        let t0 = Instant::now();
        match handler.execute(&inputs, &config) {
            Ok(_) => {
                let ns = t0.elapsed().as_nanos() as u64;
                min_ns = min_ns.min(ns);
                max_ns = max_ns.max(ns);
                sum_ns = sum_ns.saturating_add(ns);
            }
            Err(e) => return fail_case(cap, impl_id, case, iterations, warmup, e),
        }
    }
    ok_case(
        cap, impl_id, case, iterations, warmup, sum_ns, min_ns, max_ns,
    )
}

fn capture_provenance(input: &[u8]) -> BenchProvenance {
    BenchProvenance {
        recorded_at_unix: unix_now(),
        loom_version: env!("CARGO_PKG_VERSION").into(),
        rustc: option_env!("RUSTC_VERSION").unwrap_or("unknown").into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        input_fingerprint: fingerprint_bytes(input),
        notes: vec![
            "Pilot microbench — host-dependent timings; not a CI performance gate.".into(),
            "benchmark axis is pass if all cases execute without error.".into(),
        ],
    }
}

fn fingerprint_bytes(b: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    b.hash(&mut h);
    b.len().hash(&mut h);
    format!("defaulthash64:{:016x}:len={}", h.finish(), b.len())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn bench_parse(
    reg: &HandlerRegistry,
    impl_id: &str,
    case: &str,
    bytes: &[u8],
    iterations: u32,
    warmup: u32,
) -> BenchCaseResult {
    let cap = "data.json.parse@1";
    let handler = match reg.resolve(cap, Some(impl_id)) {
        Ok(h) => h,
        Err(e) => {
            return fail_case(cap, impl_id, case, iterations, warmup, e.to_string());
        }
    };
    let mut inputs = WvxValueMap::new();
    inputs.insert("bytes".into(), WvxValue::Bytes(bytes.to_vec()));
    let config = BTreeMap::new();

    for _ in 0..warmup {
        if let Err(e) = handler.execute(&inputs, &config) {
            return fail_case(cap, impl_id, case, iterations, warmup, e);
        }
    }

    let mut min_ns = u64::MAX;
    let mut max_ns = 0u64;
    let mut sum_ns = 0u64;
    for _ in 0..iterations {
        let t0 = Instant::now();
        match handler.execute(&inputs, &config) {
            Ok(_) => {
                let ns = t0.elapsed().as_nanos() as u64;
                min_ns = min_ns.min(ns);
                max_ns = max_ns.max(ns);
                sum_ns = sum_ns.saturating_add(ns);
            }
            Err(e) => return fail_case(cap, impl_id, case, iterations, warmup, e),
        }
    }
    ok_case(
        cap, impl_id, case, iterations, warmup, sum_ns, min_ns, max_ns,
    )
}

fn bench_serialize(
    reg: &HandlerRegistry,
    impl_id: &str,
    case: &str,
    value: &serde_json::Value,
    iterations: u32,
    warmup: u32,
) -> BenchCaseResult {
    let cap = "data.json.serialize@1";
    let handler = match reg.resolve(cap, Some(impl_id)) {
        Ok(h) => h,
        Err(e) => {
            return fail_case(cap, impl_id, case, iterations, warmup, e.to_string());
        }
    };
    let mut inputs = WvxValueMap::new();
    inputs.insert("value".into(), WvxValue::Json(value.clone()));
    let config = BTreeMap::new();

    for _ in 0..warmup {
        if let Err(e) = handler.execute(&inputs, &config) {
            return fail_case(cap, impl_id, case, iterations, warmup, e);
        }
    }

    let mut min_ns = u64::MAX;
    let mut max_ns = 0u64;
    let mut sum_ns = 0u64;
    for _ in 0..iterations {
        let t0 = Instant::now();
        match handler.execute(&inputs, &config) {
            Ok(_) => {
                let ns = t0.elapsed().as_nanos() as u64;
                min_ns = min_ns.min(ns);
                max_ns = max_ns.max(ns);
                sum_ns = sum_ns.saturating_add(ns);
            }
            Err(e) => return fail_case(cap, impl_id, case, iterations, warmup, e),
        }
    }
    ok_case(
        cap, impl_id, case, iterations, warmup, sum_ns, min_ns, max_ns,
    )
}

fn bench_path_set(
    reg: &HandlerRegistry,
    impl_id: &str,
    case: &str,
    input: serde_json::Value,
    path: &str,
    set_value: serde_json::Value,
    iterations: u32,
    warmup: u32,
) -> BenchCaseResult {
    let cap = "data.json.path_set@1";
    let handler = match reg.resolve(cap, Some(impl_id)) {
        Ok(h) => h,
        Err(e) => {
            return fail_case(cap, impl_id, case, iterations, warmup, e.to_string());
        }
    };
    let mut inputs = WvxValueMap::new();
    inputs.insert("value".into(), WvxValue::Json(input));
    let mut config = BTreeMap::new();
    config.insert("path".into(), serde_json::Value::String(path.into()));
    config.insert("value".into(), set_value);

    for _ in 0..warmup {
        // need fresh input each time — path_set mutates via clone inside adapter
        let mut inputs = WvxValueMap::new();
        inputs.insert(
            "value".into(),
            WvxValue::Json(serde_json::json!({"hello":"world"})),
        );
        if let Err(e) = handler.execute(&inputs, &config) {
            return fail_case(cap, impl_id, case, iterations, warmup, e);
        }
    }

    let mut min_ns = u64::MAX;
    let mut max_ns = 0u64;
    let mut sum_ns = 0u64;
    for _ in 0..iterations {
        let mut inputs = WvxValueMap::new();
        inputs.insert(
            "value".into(),
            WvxValue::Json(serde_json::json!({"hello":"world"})),
        );
        let t0 = Instant::now();
        match handler.execute(&inputs, &config) {
            Ok(_) => {
                let ns = t0.elapsed().as_nanos() as u64;
                min_ns = min_ns.min(ns);
                max_ns = max_ns.max(ns);
                sum_ns = sum_ns.saturating_add(ns);
            }
            Err(e) => return fail_case(cap, impl_id, case, iterations, warmup, e),
        }
    }
    ok_case(
        cap, impl_id, case, iterations, warmup, sum_ns, min_ns, max_ns,
    )
}

fn ok_case(
    cap: &str,
    impl_id: &str,
    case: &str,
    iterations: u32,
    warmup: u32,
    sum_ns: u64,
    min_ns: u64,
    max_ns: u64,
) -> BenchCaseResult {
    BenchCaseResult {
        capability: cap.into(),
        implementation: impl_id.into(),
        case: case.into(),
        ok: true,
        iterations,
        warmup,
        mean_ns: sum_ns / u64::from(iterations),
        min_ns,
        max_ns,
        error: None,
    }
}

fn fail_case(
    cap: &str,
    impl_id: &str,
    case: &str,
    iterations: u32,
    warmup: u32,
    error: String,
) -> BenchCaseResult {
    // Keep error code family visible if present.
    let _ = error_code_family(&error);
    BenchCaseResult {
        capability: cap.into(),
        implementation: impl_id.into(),
        case: case.into(),
        ok: false,
        iterations,
        warmup,
        mean_ns: 0,
        min_ns: 0,
        max_ns: 0,
        error: Some(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pilot_bench_smoke() {
        let report = run_pilot_bench(20, 5);
        assert!(
            report.ok,
            "bench failures: {:?}",
            report.cases.iter().filter(|c| !c.ok).collect::<Vec<_>>()
        );
        assert!(!report.cases.is_empty());
        assert!(report.cases.iter().all(|c| c.mean_ns > 0 || !c.ok));
    }
}
