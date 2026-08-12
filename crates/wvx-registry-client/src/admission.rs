//! Minimal admission policy for registry implementations (ADR-0007 / 0008).
//!
//! This is **not** full Gate E. It only checks that a declared lifecycle
//! `status` is not higher than what discrete evidence axes justify
//! (overclaim detection). Underclaim is allowed (warning).

use serde::{Deserialize, Serialize};
use wvx_ir::{AxisFact, Implementation, LifecycleStatus};

/// Rank for comparing lifecycle labels (higher = more trust claimed).
pub fn status_rank(s: LifecycleStatus) -> u8 {
    match s {
        LifecycleStatus::InventoryOnly => 0,
        LifecycleStatus::Candidate => 1,
        LifecycleStatus::Conformant => 2,
        LifecycleStatus::Admitted => 3,
    }
}

/// Highest status justified by current evidence + adapter presence.
///
/// Rules (pilot v0.1, fail-closed on overclaim):
/// - any axis `fail` → at most `candidate` (and never `admitted`)
/// - `admitted` needs: adapter, build+conformance+license+security = pass,
///   benchmark ≠ fail
/// - `conformant` needs: adapter, conformance = pass, build ≠ fail, no fail axes
/// - `candidate` needs: adapter present **or** build = pass
/// - otherwise `inventory_only`
pub fn justified_status(imp: &Implementation) -> LifecycleStatus {
    let e = &imp.evidence;
    let has_adapter = imp.adapter.is_some();
    let any_fail = [
        e.build,
        e.conformance,
        e.benchmark,
        e.license,
        e.security,
    ]
    .iter()
    .any(|a| *a == AxisFact::Fail);

    if any_fail {
        // Failed evidence blocks conformant/admitted.
        return if has_adapter || e.build == AxisFact::Pass {
            LifecycleStatus::Candidate
        } else {
            LifecycleStatus::InventoryOnly
        };
    }

    let build_ok = e.build != AxisFact::Fail; // pass, absent, unknown ok for mid ranks
    let conf_pass = e.conformance == AxisFact::Pass;
    let license_pass = e.license == AxisFact::Pass;
    let security_pass = e.security == AxisFact::Pass;
    let benchmark_ok = e.benchmark != AxisFact::Fail;
    let build_pass = e.build == AxisFact::Pass;

    if has_adapter
        && conf_pass
        && build_pass
        && license_pass
        && security_pass
        && benchmark_ok
        && e.benchmark == AxisFact::Pass
    {
        return LifecycleStatus::Admitted;
    }

    if has_adapter && conf_pass && build_ok {
        return LifecycleStatus::Conformant;
    }

    if has_adapter || build_pass {
        return LifecycleStatus::Candidate;
    }

    LifecycleStatus::InventoryOnly
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdmissionFinding {
    pub code: String,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImplementationAdmission {
    pub full_id: String,
    pub capability: String,
    pub declared: String,
    pub justified: String,
    /// Declared status is not higher than justified.
    pub ok: bool,
    pub overclaim: bool,
    pub underclaim: bool,
    pub findings: Vec<AdmissionFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdmissionReport {
    pub ok: bool,
    pub checked: usize,
    pub overclaims: usize,
    pub underclaims: usize,
    pub items: Vec<ImplementationAdmission>,
}

/// Check one implementation: overclaim → not ok; underclaim → ok with warning.
pub fn check_implementation(imp: &Implementation) -> ImplementationAdmission {
    let declared = imp.status;
    let justified = justified_status(imp);
    let overclaim = status_rank(declared) > status_rank(justified);
    let underclaim = status_rank(declared) < status_rank(justified);
    let mut findings = Vec::new();

    if overclaim {
        findings.push(AdmissionFinding {
            code: "overclaim".into(),
            severity: "error".into(),
            message: format!(
                "declared `{}` exceeds justified `{}` given evidence/adapter",
                declared.as_str(),
                justified.as_str()
            ),
        });
    }
    if underclaim {
        findings.push(AdmissionFinding {
            code: "underclaim".into(),
            severity: "warning".into(),
            message: format!(
                "declared `{}` is below justified `{}` (allowed; consider promoting)",
                declared.as_str(),
                justified.as_str()
            ),
        });
    }
    if imp.adapter.is_none() && status_rank(declared) >= status_rank(LifecycleStatus::Candidate) {
        // Only emit if not already covered by overclaim message noise
        if !overclaim {
            findings.push(AdmissionFinding {
                code: "no_adapter".into(),
                severity: "warning".into(),
                message: "no adapter declared for non-inventory status".into(),
            });
        }
    }
    if declared == LifecycleStatus::Admitted {
        findings.push(AdmissionFinding {
            code: "admitted_policy".into(),
            severity: if overclaim { "error" } else { "info" }.into(),
            message: "admitted requires human policy review beyond automated axes".into(),
        });
    }

    ImplementationAdmission {
        full_id: imp.full_id(),
        capability: imp.capability.as_key(),
        declared: declared.as_str().into(),
        justified: justified.as_str().into(),
        ok: !overclaim,
        overclaim,
        underclaim,
        findings,
    }
}

/// Audit every implementation in a list (typically a full registry dump).
pub fn audit_implementations(impls: &[Implementation]) -> AdmissionReport {
    let items: Vec<_> = impls.iter().map(check_implementation).collect();
    let overclaims = items.iter().filter(|i| i.overclaim).count();
    let underclaims = items.iter().filter(|i| i.underclaim).count();
    let ok = overclaims == 0;
    AdmissionReport {
        ok,
        checked: items.len(),
        overclaims,
        underclaims,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::{AdapterRef, CapabilityRef, ImplementationEvidence, ImplementationSource};

    fn base() -> Implementation {
        Implementation {
            id: "demo".into(),
            version: "1".into(),
            capability: CapabilityRef::new("data.json.parse", "1"),
            source: ImplementationSource {
                kind: "test".into(),
                package: "demo".into(),
                package_version: "0".into(),
                notes: None,
            },
            adapter: Some(AdapterRef {
                crate_name: "demo-adapter".into(),
                execution: "native-rust".into(),
            }),
            status: LifecycleStatus::Candidate,
            evidence: ImplementationEvidence::default(),
            notes: None,
        }
    }

    #[test]
    fn conformant_when_conformance_pass() {
        let mut i = base();
        i.evidence.conformance = AxisFact::Pass;
        i.evidence.build = AxisFact::Pass;
        i.status = LifecycleStatus::Conformant;
        let r = check_implementation(&i);
        assert!(r.ok, "{:?}", r.findings);
        assert_eq!(r.justified, "conformant");
    }

    #[test]
    fn overclaim_admitted_without_security() {
        let mut i = base();
        i.evidence.conformance = AxisFact::Pass;
        i.evidence.build = AxisFact::Pass;
        i.evidence.license = AxisFact::Pass;
        i.evidence.security = AxisFact::Absent;
        i.evidence.benchmark = AxisFact::Pass;
        i.status = LifecycleStatus::Admitted;
        let r = check_implementation(&i);
        assert!(!r.ok);
        assert!(r.overclaim);
        assert_eq!(r.justified, "conformant");
    }

    #[test]
    fn fail_axis_blocks_conformant() {
        let mut i = base();
        i.evidence.conformance = AxisFact::Pass;
        i.evidence.build = AxisFact::Fail;
        i.status = LifecycleStatus::Conformant;
        let r = check_implementation(&i);
        assert!(!r.ok);
        assert_eq!(r.justified, "candidate");
    }

    #[test]
    fn pilot_style_conformant_ok() {
        let mut i = base();
        i.status = LifecycleStatus::Conformant;
        i.evidence = ImplementationEvidence {
            build: AxisFact::Pass,
            conformance: AxisFact::Pass,
            benchmark: AxisFact::Absent,
            license: AxisFact::Pass,
            security: AxisFact::Absent,
        };
        assert!(check_implementation(&i).ok);
    }
}
