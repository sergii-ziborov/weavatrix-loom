//! VerifiedImplementation — release-safe handle after artifact verification.
//!
//! Release compile must accept only these, not raw manifests.

use crate::evidence_artifact::{
    load_artifact, verify_artifact, ArtifactCheck, EvidenceArtifact,
};
use crate::RegistryError;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use wvx_ir::{Implementation, LifecycleStatus};

/// An implementation whose evidence artifact has been verified under a registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedImplementation {
    pub implementation: Implementation,
    pub artifact: EvidenceArtifact,
    pub check: ArtifactCheck,
    pub verified_at_unix: u64,
}

impl VerifiedImplementation {
    pub fn full_id(&self) -> String {
        self.implementation.full_id()
    }

    pub fn capability_key(&self) -> String {
        self.implementation.capability.as_key()
    }

    /// Lifecycle claimed after verification (from check.justified).
    pub fn justified_status(&self) -> LifecycleStatus {
        match self.check.justified.as_str() {
            "admitted" => LifecycleStatus::Admitted,
            "conformant" => LifecycleStatus::Conformant,
            "candidate" => LifecycleStatus::Candidate,
            _ => LifecycleStatus::InventoryOnly,
        }
    }
}

/// Verify an implementation's on-disk artifact and return a release handle.
///
/// Fail-closed: missing/bad artifact → Err or ok=false check.
pub fn verify_implementation(
    registry_root: &Path,
    imp: &Implementation,
) -> Result<VerifiedImplementation, RegistryError> {
    let check = verify_artifact(registry_root, imp);
    if !check.ok {
        return Err(RegistryError::Parse(
            registry_root.to_path_buf(),
            format!(
                "implementation `{}` not verified: {}",
                imp.full_id(),
                check.findings.join("; ")
            ),
        ));
    }
    let path = crate::evidence_artifact::artifact_path(registry_root, imp);
    let artifact = load_artifact(&path)?;
    Ok(VerifiedImplementation {
        implementation: imp.clone(),
        artifact,
        check,
        verified_at_unix: unix_now(),
    })
}

/// Verify all implementations that declare status ≥ conformant.
pub fn verify_all_conformant(
    registry_root: &Path,
    impls: &[Implementation],
) -> Result<Vec<VerifiedImplementation>, RegistryError> {
    let mut out = Vec::new();
    for imp in impls {
        if matches!(
            imp.status,
            LifecycleStatus::Conformant | LifecycleStatus::Admitted
        ) {
            out.push(verify_implementation(registry_root, imp)?);
        }
    }
    Ok(out)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalRegistry;

    #[test]
    fn verifies_serde_json_sample() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../registry-dev");
        let reg = LocalRegistry::open(&root).unwrap();
        let imp = reg
            .find_implementation("serde-json.parse-owned@1")
            .unwrap()
            .expect("impl");
        let v = verify_implementation(&root, &imp).expect("verified");
        assert!(v.check.ok);
        assert_eq!(v.artifact.schema_version, crate::EVIDENCE_SCHEMA);
    }
}
