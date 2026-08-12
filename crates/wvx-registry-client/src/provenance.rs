//! Provenance records for Gate E (registry trust).
//!
//! Sidecar JSON next to an implementation key — not a readiness score.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use wvx_ir::Implementation;

use crate::RegistryError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvenanceRecord {
    pub implementation_id: String,
    pub capability: String,
    pub recorded_at_unix: u64,
    pub source_kind: String,
    pub package: String,
    pub package_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    pub status: String,
    pub evidence: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub host: HostProvenance,
    #[serde(default)]
    pub review: Option<HumanReview>,
    #[serde(default)]
    pub bench_fingerprint: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HostProvenance {
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub arch: String,
    #[serde(default)]
    pub loom_version: String,
    #[serde(default)]
    pub rustc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HumanReview {
    pub reviewer: String,
    pub ack: String,
    pub security_ack: String,
    pub reason: String,
    pub reviewed_at_unix: u64,
}

pub fn provenance_path(registry_root: &Path, full_id: &str) -> PathBuf {
    let safe = full_id.replace(['/', '\\', ':'], "_");
    registry_root.join("evidence").join(format!("{safe}.provenance.json"))
}

pub fn write_provenance(registry_root: &Path, record: &ProvenanceRecord) -> Result<PathBuf, RegistryError> {
    let path = provenance_path(registry_root, &record.implementation_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| RegistryError::Io(parent.to_path_buf(), e))?;
    }
    let text = serde_json::to_string_pretty(record)
        .map_err(|e| RegistryError::Parse(path.clone(), e.to_string()))?;
    fs::write(&path, text + "\n").map_err(|e| RegistryError::Io(path.clone(), e))?;
    Ok(path)
}

pub fn read_provenance(
    registry_root: &Path,
    full_id: &str,
) -> Result<Option<ProvenanceRecord>, RegistryError> {
    let path = provenance_path(registry_root, full_id);
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|e| RegistryError::Io(path.clone(), e))?;
    let rec = serde_json::from_str(&text)
        .map_err(|e| RegistryError::Parse(path, e.to_string()))?;
    Ok(Some(rec))
}

pub fn provenance_from_impl(
    imp: &Implementation,
    review: Option<HumanReview>,
    bench_fingerprint: Option<String>,
    notes: Vec<String>,
) -> ProvenanceRecord {
    use wvx_ir::AxisFact;
    let mut evidence = std::collections::BTreeMap::new();
    let push = |m: &mut std::collections::BTreeMap<String, String>, k: &str, v: AxisFact| {
        m.insert(k.into(), v.as_str().into());
    };
    push(&mut evidence, "build", imp.evidence.build);
    push(&mut evidence, "conformance", imp.evidence.conformance);
    push(&mut evidence, "benchmark", imp.evidence.benchmark);
    push(&mut evidence, "license", imp.evidence.license);
    push(&mut evidence, "security", imp.evidence.security);

    ProvenanceRecord {
        implementation_id: imp.full_id(),
        capability: imp.capability.as_key(),
        recorded_at_unix: unix_now(),
        source_kind: imp.source.kind.clone(),
        package: imp.source.package.clone(),
        package_version: imp.source.package_version.clone(),
        adapter: imp.adapter.as_ref().map(|a| a.crate_name.clone()),
        status: imp.status.as_str().into(),
        evidence,
        host: HostProvenance {
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            loom_version: env!("CARGO_PKG_VERSION").into(),
            rustc: option_env!("RUSTC_VERSION").unwrap_or("unknown").into(),
        },
        review,
        bench_fingerprint,
        notes,
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
