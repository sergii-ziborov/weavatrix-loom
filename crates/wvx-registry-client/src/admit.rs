//! Human admission path for Gate E (pilot).
//!
//! Fail-closed: requires human acks + successful bench evidence before
//! promoting an implementation to `admitted`. Does not auto-admit CI.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use wvx_ir::{AxisFact, Implementation, LifecycleStatus};

use crate::admission::{check_implementation, justified_status};
use crate::provenance::{
    provenance_from_impl, write_provenance, HumanReview, ProvenanceRecord,
};
use crate::RegistryError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdmitRequest {
    pub implementation_id: String,
    pub reviewer: String,
    /// Free-text human acknowledgment (must be non-empty).
    pub human_ack: String,
    /// Explicit security review acknowledgment for pilot (must be non-empty).
    pub security_ack: String,
    pub reason: String,
    /// Fingerprint / summary from a successful bench report.
    pub bench_fingerprint: String,
    /// When true, rewrite the implementation manifest under registry-dev.
    #[serde(default)]
    pub apply: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct AdmitResult {
    pub ok: bool,
    pub implementation_id: String,
    pub previous_status: String,
    pub new_status: String,
    pub justified: String,
    pub provenance_path: Option<String>,
    pub manifest_path: Option<String>,
    pub findings: Vec<String>,
    pub implementation: Implementation,
}

/// Apply Gate E pilot admission: set benchmark+security pass, status admitted if justified.
pub fn admit_implementation(
    registry_root: &Path,
    mut imp: Implementation,
    req: &AdmitRequest,
) -> Result<AdmitResult, RegistryError> {
    let mut findings = Vec::new();
    let previous = imp.status;

    if req.human_ack.trim().len() < 8 {
        return Ok(AdmitResult {
            ok: false,
            implementation_id: imp.full_id(),
            previous_status: previous.as_str().into(),
            new_status: previous.as_str().into(),
            justified: justified_status(&imp).as_str().into(),
            provenance_path: None,
            manifest_path: None,
            findings: vec![
                "human_ack too short — document the review (min 8 chars)".into(),
            ],
            implementation: imp,
        });
    }
    if req.security_ack.trim().len() < 8 {
        return Ok(AdmitResult {
            ok: false,
            implementation_id: imp.full_id(),
            previous_status: previous.as_str().into(),
            new_status: previous.as_str().into(),
            justified: justified_status(&imp).as_str().into(),
            provenance_path: None,
            manifest_path: None,
            findings: vec![
                "security_ack too short — pilot requires explicit security acknowledgment".into(),
            ],
            implementation: imp,
        });
    }
    if req.reviewer.trim().is_empty() {
        return Ok(AdmitResult {
            ok: false,
            implementation_id: imp.full_id(),
            previous_status: previous.as_str().into(),
            new_status: previous.as_str().into(),
            justified: justified_status(&imp).as_str().into(),
            provenance_path: None,
            manifest_path: None,
            findings: vec!["reviewer is required".into()],
            implementation: imp,
        });
    }
    if req.bench_fingerprint.trim().is_empty() {
        return Ok(AdmitResult {
            ok: false,
            implementation_id: imp.full_id(),
            previous_status: previous.as_str().into(),
            new_status: previous.as_str().into(),
            justified: justified_status(&imp).as_str().into(),
            provenance_path: None,
            manifest_path: None,
            findings: vec![
                "bench_fingerprint required — run `wvx bench` and pass report fingerprint".into(),
            ],
            implementation: imp,
        });
    }

    // Promote axes that human+bench supply for Gate E pilot.
    if imp.evidence.build == AxisFact::Absent {
        imp.evidence.build = AxisFact::Pass;
        findings.push("set evidence.build=pass (pilot adapter present)".into());
    }
    if imp.evidence.conformance != AxisFact::Pass {
        findings.push(
            "conformance is not pass — admit requires Gate A vectors first".into(),
        );
        return Ok(AdmitResult {
            ok: false,
            implementation_id: imp.full_id(),
            previous_status: previous.as_str().into(),
            new_status: previous.as_str().into(),
            justified: justified_status(&imp).as_str().into(),
            provenance_path: None,
            manifest_path: None,
            findings,
            implementation: imp,
        });
    }
    if imp.adapter.is_none() {
        findings.push("no adapter — cannot admit".into());
        return Ok(AdmitResult {
            ok: false,
            implementation_id: imp.full_id(),
            previous_status: previous.as_str().into(),
            new_status: previous.as_str().into(),
            justified: justified_status(&imp).as_str().into(),
            provenance_path: None,
            manifest_path: None,
            findings,
            implementation: imp,
        });
    }

    imp.evidence.benchmark = AxisFact::Pass;
    imp.evidence.security = AxisFact::Pass;
    if imp.evidence.license == AxisFact::Absent {
        imp.evidence.license = AxisFact::Pass;
        findings.push("set evidence.license=pass from package metadata / pilot policy".into());
    }
    imp.status = LifecycleStatus::Admitted;
    if let Some(n) = imp.notes.as_mut() {
        n.push_str(" | Gate E pilot admit");
    } else {
        imp.notes = Some(format!(
            "Gate E pilot admit by {} — {}",
            req.reviewer.trim(),
            req.reason.trim()
        ));
    }

    let just = justified_status(&imp);
    if just != LifecycleStatus::Admitted {
        findings.push(format!(
            "after axis updates justified is `{}`, not admitted — policy blocked",
            just.as_str()
        ));
        // rollback status for response clarity
        imp.status = previous;
        return Ok(AdmitResult {
            ok: false,
            implementation_id: imp.full_id(),
            previous_status: previous.as_str().into(),
            new_status: previous.as_str().into(),
            justified: just.as_str().into(),
            provenance_path: None,
            manifest_path: None,
            findings,
            implementation: imp,
        });
    }

    // Overclaim check on the new state
    let check = check_implementation(&imp);
    if !check.ok {
        findings.push("post-admit overclaim check failed".into());
        for f in check.findings {
            findings.push(format!("{}: {}", f.code, f.message));
        }
        imp.status = previous;
        return Ok(AdmitResult {
            ok: false,
            implementation_id: imp.full_id(),
            previous_status: previous.as_str().into(),
            new_status: previous.as_str().into(),
            justified: just.as_str().into(),
            provenance_path: None,
            manifest_path: None,
            findings,
            implementation: imp,
        });
    }

    let review = HumanReview {
        reviewer: req.reviewer.trim().into(),
        ack: req.human_ack.trim().into(),
        security_ack: req.security_ack.trim().into(),
        reason: req.reason.trim().into(),
        reviewed_at_unix: unix_now(),
    };
    let prov = provenance_from_impl(
        &imp,
        Some(review),
        Some(req.bench_fingerprint.clone()),
        vec![
            "Gate E pilot human admission".into(),
            "Timings are host-dependent; bench pass = executions ok.".into(),
        ],
    );
    let prov_path = write_provenance(registry_root, &prov)?;
    findings.push(format!("wrote {}", prov_path.display()));

    let mut manifest_path = None;
    if req.apply {
        let path = write_implementation_manifest(registry_root, &imp)?;
        findings.push(format!("updated manifest {}", path.display()));
        manifest_path = Some(path.display().to_string());
    } else {
        findings.push(
            "dry-run: pass --apply to write implementation manifest (status=admitted)".into(),
        );
    }

    // Also mirror under .lab/admissions for local lab trail (best-effort).
    let _ = write_lab_copy(registry_root, &imp, &prov);

    Ok(AdmitResult {
        ok: true,
        implementation_id: imp.full_id(),
        previous_status: previous.as_str().into(),
        new_status: imp.status.as_str().into(),
        justified: just.as_str().into(),
        provenance_path: Some(prov_path.display().to_string()),
        manifest_path,
        findings,
        implementation: imp,
    })
}

fn write_implementation_manifest(
    registry_root: &Path,
    imp: &Implementation,
) -> Result<PathBuf, RegistryError> {
    let dir = registry_root.join("implementations");
    fs::create_dir_all(&dir).map_err(|e| RegistryError::Io(dir.clone(), e))?;
    let path = dir.join(format!("{}.json", imp.full_id()));
    let text = serde_json::to_string_pretty(imp)
        .map_err(|e| RegistryError::Parse(path.clone(), e.to_string()))?;
    fs::write(&path, text + "\n").map_err(|e| RegistryError::Io(path.clone(), e))?;
    Ok(path)
}

fn write_lab_copy(
    registry_root: &Path,
    imp: &Implementation,
    prov: &ProvenanceRecord,
) -> Result<(), RegistryError> {
    // Prefer repo-level .lab next to registry-dev parent.
    let lab = registry_root
        .parent()
        .unwrap_or(registry_root)
        .join(".lab")
        .join("admissions");
    fs::create_dir_all(&lab).map_err(|e| RegistryError::Io(lab.clone(), e))?;
    let safe = imp.full_id().replace(['/', '\\', ':'], "_");
    let path = lab.join(format!("{safe}.json"));
    let body = serde_json::json!({
        "implementation": imp,
        "provenance": prov,
    });
    let text = serde_json::to_string_pretty(&body)
        .map_err(|e| RegistryError::Parse(path.clone(), e.to_string()))?;
    fs::write(&path, text + "\n").map_err(|e| RegistryError::Io(path, e))?;
    Ok(())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::{AdapterRef, CapabilityRef, ImplementationEvidence, ImplementationSource};

    fn sample_conformant() -> Implementation {
        Implementation {
            id: "serde-json.parse-owned".into(),
            version: "1".into(),
            capability: CapabilityRef::new("data.json.parse", "1"),
            source: ImplementationSource {
                kind: "crates-io".into(),
                package: "serde_json".into(),
                package_version: "1".into(),
                notes: None,
            },
            adapter: Some(AdapterRef {
                crate_name: "wvx-adapter-serde-json-parse".into(),
                execution: "native-rust".into(),
            }),
            status: LifecycleStatus::Conformant,
            evidence: ImplementationEvidence {
                build: AxisFact::Pass,
                conformance: AxisFact::Pass,
                benchmark: AxisFact::Absent,
                license: AxisFact::Pass,
                security: AxisFact::Absent,
            },
            notes: Some("test".into()),
        }
    }

    #[test]
    fn admit_requires_acks() {
        let dir = std::env::temp_dir().join(format!("wvx-admit-{}", unix_now()));
        let _ = fs::create_dir_all(&dir);
        let imp = sample_conformant();
        let req = AdmitRequest {
            implementation_id: imp.full_id(),
            reviewer: "tester".into(),
            human_ack: "short".into(),
            security_ack: "also short".into(),
            reason: "test".into(),
            bench_fingerprint: "x".into(),
            apply: false,
        };
        let r = admit_implementation(&dir, imp, &req).unwrap();
        assert!(!r.ok);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn admit_dry_run_succeeds_with_acks() {
        let dir = std::env::temp_dir().join(format!("wvx-admit-ok-{}", unix_now()));
        let _ = fs::create_dir_all(dir.join("evidence"));
        let imp = sample_conformant();
        let req = AdmitRequest {
            implementation_id: imp.full_id(),
            reviewer: "Sergii Ziborov".into(),
            human_ack: "Reviewed pilot Gate E evidence for serde parse".into(),
            security_ack: "Pilot-only security posture accepted for lab admit".into(),
            reason: "Demonstrate Gate E human admit path".into(),
            bench_fingerprint: "bench-ok:test".into(),
            apply: false,
        };
        let r = admit_implementation(&dir, imp, &req).unwrap();
        assert!(r.ok, "{:?}", r.findings);
        assert_eq!(r.new_status, "admitted");
        assert!(r.provenance_path.is_some());
        let _ = fs::remove_dir_all(&dir);
    }
}
