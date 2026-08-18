//! P2: signed attestations, SBOM, and an append-only transparency log.
//!
//! Not a hosted PKI. Signatures reuse HMAC-SHA256 (`WVX_PROMOTION_HMAC_KEY`).
//! The transparency log is a hash-chained JSONL under `transparency/log.jsonl`.

use crate::evidence_artifact::{sha256_hex, EvidenceArtifact};
use crate::signed::{hmac_sha256, promotion_hmac_key};
use crate::RegistryError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use wvx_ir::Implementation;

pub const ATTESTATION_SCHEMA: &str = "wvx.attestation.v0.1";
pub const SBOM_SCHEMA: &str = "wvx.sbom.v0.1";
pub const TRANSPARENCY_GENESIS: &str = "sha256:genesis";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedAttestation {
    pub schema_version: String,
    pub implementation_id: String,
    pub profile_id: String,
    pub artifact_digest: String,
    pub source_digest: String,
    pub sbom_digest: String,
    pub recorded_at_unix: u64,
    pub payload_digest: String,
    pub signature: String,
}

impl SignedAttestation {
    fn canonical_payload(&self) -> String {
        serde_json::json!({
            "schema_version": self.schema_version,
            "implementation_id": self.implementation_id,
            "profile_id": self.profile_id,
            "artifact_digest": self.artifact_digest,
            "source_digest": self.source_digest,
            "sbom_digest": self.sbom_digest,
            "recorded_at_unix": self.recorded_at_unix,
        })
        .to_string()
    }
}

/// Sign an attestation over an evidence artifact + SBOM digest.
pub fn sign_attestation(
    impl_id: &str,
    profile_id: &str,
    artifact: &EvidenceArtifact,
    sbom_digest: &str,
    recorded_at_unix: u64,
    key: &[u8],
) -> SignedAttestation {
    let mut att = SignedAttestation {
        schema_version: ATTESTATION_SCHEMA.into(),
        implementation_id: impl_id.into(),
        profile_id: profile_id.into(),
        artifact_digest: if !artifact.digests.subject.trim().is_empty() {
            artifact.digests.subject.clone()
        } else {
            artifact.subject_digest.clone()
        },
        source_digest: artifact.digests.implementation_source_tree.clone(),
        sbom_digest: sbom_digest.into(),
        recorded_at_unix,
        payload_digest: String::new(),
        signature: String::new(),
    };
    let payload = att.canonical_payload();
    att.payload_digest = sha256_hex(payload.as_bytes());
    att.signature = hmac_sha256(key, att.payload_digest.as_bytes());
    att
}

pub fn verify_attestation(
    att: &SignedAttestation,
    expected_impl: &str,
) -> Result<(), RegistryError> {
    if att.schema_version != ATTESTATION_SCHEMA {
        return Err(err(format!(
            "attestation schema `{}` != `{ATTESTATION_SCHEMA}`",
            att.schema_version
        )));
    }
    if att.implementation_id != expected_impl {
        return Err(err(format!(
            "attestation impl `{}` != `{expected_impl}`",
            att.implementation_id
        )));
    }
    let recomputed = sha256_hex(att.canonical_payload().as_bytes());
    if recomputed != att.payload_digest {
        return Err(err("attestation payload_digest mismatch"));
    }
    let Some(key) = promotion_hmac_key() else {
        return Err(err(
            "attestation rejected: WVX_PROMOTION_HMAC_KEY is not set (fail-closed)",
        ));
    };
    let expect = hmac_sha256(&key, att.payload_digest.as_bytes());
    if expect != att.signature {
        return Err(err("attestation HMAC signature mismatch"));
    }
    Ok(())
}

// ─── SBOM ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SbomComponent {
    pub name: String,
    #[serde(default)]
    pub version: String,
    /// `upstream` | `adapter` | `capability` | `lock`
    pub kind: String,
    /// Cargo PURL when name+version look like a crate (`pkg:cargo/name@version`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purl: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SoftwareBill {
    pub schema_version: String,
    /// CycloneDX-shaped labels — not a full 1.5 document.
    #[serde(default)]
    pub bom_format: String,
    #[serde(default)]
    pub spec_version: String,
    pub implementation_id: String,
    pub components: Vec<SbomComponent>,
}

fn cargo_purl(name: &str, version: &str) -> Option<String> {
    if name.trim().is_empty() || version.trim().is_empty() || version == "1" && name.contains('.') {
        return None;
    }
    if !version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("pkg:cargo/{name}@{version}"))
}

impl SoftwareBill {
    pub fn digest(&self) -> String {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        sha256_hex(&bytes)
    }
}

/// Minimal SBOM from the implementation identity (not a cargo-metadata dump).
pub fn sbom_from_implementation(imp: &Implementation) -> SoftwareBill {
    let mut components = Vec::new();
    if !imp.source.package.trim().is_empty() {
        components.push(SbomComponent {
            name: imp.source.package.clone(),
            version: imp.source.package_version.clone(),
            kind: "upstream".into(),
            purl: cargo_purl(&imp.source.package, &imp.source.package_version),
            digest: None,
        });
    }
    if let Some(ad) = &imp.adapter {
        components.push(SbomComponent {
            name: ad.crate_name.clone(),
            version: String::new(),
            kind: "adapter".into(),
            purl: None,
            digest: None,
        });
    }
    components.push(SbomComponent {
        name: format!("{}@{}", imp.capability.id, imp.capability.version),
        version: imp.capability.version.clone(),
        kind: "capability".into(),
        purl: None,
        digest: None,
    });
    let mut bill = SoftwareBill {
        schema_version: SBOM_SCHEMA.into(),
        bom_format: "CycloneDX".into(),
        spec_version: "1.5".into(),
        implementation_id: imp.full_id(),
        components,
    };
    if let Some(lock) = find_cargo_lock() {
        enrich_sbom_from_lock(&mut bill, &lock);
    }
    bill
}

fn find_cargo_lock() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..6 {
        let p = dir.join("Cargo.lock");
        if p.is_file() {
            return Some(p);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Add lockfile packages that match already-listed component names (not the whole graph).
fn enrich_sbom_from_lock(bill: &mut SoftwareBill, lock_path: &Path) {
    let Ok(text) = fs::read_to_string(lock_path) else {
        return;
    };
    let want: Vec<String> = bill
        .components
        .iter()
        .map(|c| c.name.replace('_', "-").to_ascii_lowercase())
        .collect();
    let mut name = String::new();
    let mut version = String::new();
    let mut checksum = None;
    let flush = |bill: &mut SoftwareBill, name: &str, version: &str, checksum: &Option<String>| {
        if name.is_empty() {
            return;
        }
        let key = name.replace('_', "-").to_ascii_lowercase();
        if want
            .iter()
            .any(|w| key == *w || key.contains(w) || w.contains(&key))
        {
            bill.components.push(SbomComponent {
                name: name.to_string(),
                version: version.to_string(),
                kind: "lock".into(),
                purl: cargo_purl(name, version),
                digest: checksum.clone(),
            });
        }
    };
    for line in text.lines() {
        let t = line.trim();
        if t == "[[package]]" {
            flush(bill, &name, &version, &checksum);
            name.clear();
            version.clear();
            checksum = None;
            continue;
        }
        if let Some(v) = t.strip_prefix("name = \"") {
            name = v.trim_end_matches('"').into();
        } else if let Some(v) = t.strip_prefix("version = \"") {
            version = v.trim_end_matches('"').into();
        } else if let Some(v) = t.strip_prefix("checksum = \"") {
            checksum = Some(format!("sha256:{}", v.trim_end_matches('"')));
        }
    }
    flush(bill, &name, &version, &checksum);
}

// ─── Transparency log ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransparencyKind {
    Promote,
    Attest,
    Requal,
    Sigstore,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransparencyEntry {
    pub seq: u64,
    pub kind: TransparencyKind,
    pub implementation_id: String,
    pub payload_digest: String,
    pub prev_hash: String,
    pub entry_hash: String,
    pub recorded_at_unix: u64,
}

pub fn transparency_path(registry_root: &Path) -> PathBuf {
    registry_root.join("transparency").join("log.jsonl")
}

pub fn read_transparency_log(
    registry_root: &Path,
) -> Result<Vec<TransparencyEntry>, RegistryError> {
    let path = transparency_path(registry_root);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path).map_err(|e| RegistryError::Io(path.clone(), e))?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let e: TransparencyEntry = serde_json::from_str(line)
            .map_err(|e| RegistryError::Parse(path.clone(), format!("line {}: {e}", i + 1)))?;
        out.push(e);
    }
    Ok(out)
}

pub fn verify_transparency_log(entries: &[TransparencyEntry]) -> Result<(), RegistryError> {
    let mut prev = TRANSPARENCY_GENESIS.to_string();
    for (i, e) in entries.iter().enumerate() {
        if e.seq != (i as u64) + 1 {
            return Err(err(format!(
                "transparency seq {} at index {} (expected {})",
                e.seq,
                i,
                i + 1
            )));
        }
        if e.prev_hash != prev {
            return Err(err(format!(
                "transparency prev_hash break at seq {}",
                e.seq
            )));
        }
        let expect = entry_hash(
            e.seq,
            e.kind,
            &e.implementation_id,
            &e.payload_digest,
            &e.prev_hash,
        );
        if expect != e.entry_hash {
            return Err(err(format!(
                "transparency entry_hash mismatch at seq {}",
                e.seq
            )));
        }
        prev = e.entry_hash.clone();
    }
    Ok(())
}

pub fn append_transparency(
    registry_root: &Path,
    kind: TransparencyKind,
    implementation_id: &str,
    payload_digest: &str,
    recorded_at_unix: u64,
) -> Result<TransparencyEntry, RegistryError> {
    let log = read_transparency_log(registry_root)?;
    verify_transparency_log(&log)?;
    let prev = log
        .last()
        .map(|e| e.entry_hash.clone())
        .unwrap_or_else(|| TRANSPARENCY_GENESIS.into());
    let seq = (log.len() as u64) + 1;
    let entry_hash = entry_hash(seq, kind, implementation_id, payload_digest, &prev);
    let entry = TransparencyEntry {
        seq,
        kind,
        implementation_id: implementation_id.into(),
        payload_digest: payload_digest.into(),
        prev_hash: prev,
        entry_hash,
        recorded_at_unix,
    };
    let path = transparency_path(registry_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| RegistryError::Io(parent.to_path_buf(), e))?;
    }
    let line = serde_json::to_string(&entry)
        .map_err(|e| RegistryError::Parse(path.clone(), e.to_string()))?;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| RegistryError::Io(path.clone(), e))?;
    writeln!(f, "{line}").map_err(|e| RegistryError::Io(path, e))?;
    Ok(entry)
}

fn entry_hash(
    seq: u64,
    kind: TransparencyKind,
    impl_id: &str,
    payload: &str,
    prev: &str,
) -> String {
    let kind_s = match kind {
        TransparencyKind::Promote => "promote",
        TransparencyKind::Attest => "attest",
        TransparencyKind::Requal => "requal",
        TransparencyKind::Sigstore => "sigstore",
    };
    sha256_hex(format!("{seq}|{kind_s}|{impl_id}|{payload}|{prev}").as_bytes())
}

fn err(msg: impl Into<String>) -> RegistryError {
    RegistryError::Parse(Path::new("<attestation>").to_path_buf(), msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::{CapabilityRef, ImplementationSource};

    fn sample_imp() -> Implementation {
        Implementation {
            id: "demo.parse".into(),
            version: "1".into(),
            capability: CapabilityRef {
                id: "data.json.parse".into(),
                version: "1".into(),
            },
            source: ImplementationSource {
                kind: "crates.io".into(),
                package: "serde_json".into(),
                package_version: "1.0.0".into(),
                notes: None,
            },
            adapter: Some(wvx_ir::AdapterRef {
                crate_name: "wvx-adapters".into(),
                execution: "native-rust".into(),
            }),
            status: wvx_ir::LifecycleStatus::Candidate,
            evidence: Default::default(),
            evidence_artifact: None,
            notes: None,
            sdk: None,
            source_ref: None,
            conformance_profile: Some("json-rfc8259-core-v1".into()),
        }
    }

    #[test]
    fn sbom_lists_upstream_adapter_capability() {
        let bill = sbom_from_implementation(&sample_imp());
        assert_eq!(bill.schema_version, SBOM_SCHEMA);
        assert!(bill.components.iter().any(|c| c.kind == "upstream"));
        assert!(bill.components.iter().any(|c| c.kind == "capability"));
        assert!(bill.digest().starts_with("sha256:"));
    }

    #[test]
    fn attestation_roundtrip_hmac() {
        std::env::set_var("WVX_PROMOTION_HMAC_KEY", "test-attestation-key");
        let art = EvidenceArtifact {
            schema_version: crate::EVIDENCE_SCHEMA.into(),
            implementation_id: "demo.parse@1".into(),
            capability_key: "data.json.parse@1".into(),
            conformance_profile: "json-rfc8259-core-v1".into(),
            subject_digest: "sha256:abc".into(),
            profile_suite_digest: String::new(),
            suite_results: vec![],
            axes: Default::default(),
            recorded_at_unix: 1,
            notes: vec![],
            digests: crate::evidence_artifact::EvidenceDigests {
                subject: "sha256:abc".into(),
                implementation_source_tree: "sha256:src".into(),
                ..Default::default()
            },
            environment: Default::default(),
            case_results: vec![],
        };
        let key = b"test-attestation-key";
        let att = sign_attestation(
            "demo.parse@1",
            "json-rfc8259-core-v1",
            &art,
            "sha256:sbom",
            1,
            key,
        );
        verify_attestation(&att, "demo.parse@1").unwrap();
        let mut bad = att.clone();
        bad.signature = "nope".into();
        assert!(verify_attestation(&bad, "demo.parse@1").is_err());
    }

    #[test]
    fn transparency_hash_chain() {
        let dir = std::env::temp_dir().join(format!(
            "wvx-transparency-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        append_transparency(&dir, TransparencyKind::Promote, "a@1", "sha256:1", 1).unwrap();
        append_transparency(&dir, TransparencyKind::Attest, "a@1", "sha256:2", 2).unwrap();
        let log = read_transparency_log(&dir).unwrap();
        assert_eq!(log.len(), 2);
        verify_transparency_log(&log).unwrap();
        let mut broken = log.clone();
        broken[1].prev_hash = "sha256:tamper".into();
        assert!(verify_transparency_log(&broken).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
