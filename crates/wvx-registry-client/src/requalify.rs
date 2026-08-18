//! Incremental requalification when upstream version, source digest, profile,
//! or toolchain drift relative to the last verified artifact.
//!
//! Does **not** auto-admit. `stale=true` means promote / suite must be re-run.

use crate::admission::{check_implementation, ImplementationAdmission};
use crate::evidence_artifact::{
    artifact_path, capture_environment, compute_digests, load_artifact, DigestContext,
};
use crate::{LocalRegistry, RegistryError};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use wvx_ir::{AxisFact, EvidenceRecord, LifecycleStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequalTrigger {
    pub kind: String,
    pub previous: String,
    pub current: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequalifyReport {
    pub implementation_id: String,
    pub ok: bool,
    /// True when last artifact no longer matches live source/profile/toolchain.
    pub stale: bool,
    pub previous_status: String,
    pub justified_status: String,
    pub overclaim: bool,
    pub triggers: Vec<RequalTrigger>,
    pub evidence: EvidenceRecord,
    pub notes: Vec<String>,
    pub recorded_at_unix: u64,
}

/// Re-evaluate an implementation against admission rules (requal on new version).
pub fn requalify_implementation(
    reg: &LocalRegistry,
    full_id: &str,
) -> Result<RequalifyReport, RegistryError> {
    let imp = reg
        .find_implementation(full_id)?
        .ok_or_else(|| RegistryError::Parse(reg.root.clone(), format!("unknown impl {full_id}")))?;

    let admission: ImplementationAdmission = check_implementation(&imp);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let evidence = EvidenceRecord {
        implementation_id: imp.full_id(),
        capability_key: format!("{}@{}", imp.capability.id, imp.capability.version),
        lifecycle: imp.status,
        axes: imp.evidence.clone(),
        bench: None,
        notes: vec![
            format!("requalify at unix={now}"),
            format!("declared_status={}", imp.status.as_str()),
            format!("justified_status={}", admission.justified),
        ],
    };

    let mut notes: Vec<String> = admission
        .findings
        .iter()
        .map(|f| format!("{}: {}", f.code, f.message))
        .collect();
    notes.push(
        "Incremental requalification: compare last artifact vs live source/profile/toolchain."
            .into(),
    );
    if admission.overclaim {
        notes.push("OVERCLAIM: declared lifecycle not justified by evidence axes.".into());
    }
    if imp.evidence.conformance == AxisFact::Pass {
        notes.push("conformance axis pass — still not production admission".into());
    }

    let mut triggers = Vec::new();
    let art_path = artifact_path(&reg.root, &imp);
    if art_path.is_file() {
        let art = load_artifact(&art_path)?;
        let live = compute_digests(&reg.root, &imp, None, &DigestContext::default());
        let env = capture_environment("wvx-registry-client");
        push_trigger(
            &mut triggers,
            "upstream_version",
            &art.digests.upstream_package,
            &live.upstream_package,
        );
        push_trigger(
            &mut triggers,
            "source_digest",
            &art.digests.implementation_source_tree,
            &live.implementation_source_tree,
        );
        let prev_profile = if art.conformance_profile.is_empty() {
            "-"
        } else {
            art.conformance_profile.as_str()
        };
        let cur_profile = imp.conformance_profile.as_deref().unwrap_or("-");
        if prev_profile != cur_profile {
            triggers.push(RequalTrigger {
                kind: "profile".into(),
                previous: prev_profile.into(),
                current: cur_profile.into(),
            });
        }
        push_trigger(
            &mut triggers,
            "toolchain",
            &art.environment.toolchain,
            &env.toolchain,
        );
    } else {
        notes.push("no evidence artifact — cannot detect incremental drift".into());
    }

    let stale = !triggers.is_empty();
    if stale {
        notes.push(format!(
            "STALE: {} trigger(s) — re-run promote / profile suite",
            triggers.len()
        ));
    }

    Ok(RequalifyReport {
        implementation_id: imp.full_id(),
        ok: !admission.overclaim && !stale,
        stale,
        previous_status: imp.status.as_str().into(),
        justified_status: admission.justified.clone(),
        overclaim: admission.overclaim,
        triggers,
        evidence,
        notes,
        recorded_at_unix: now,
    })
}

fn push_trigger(out: &mut Vec<RequalTrigger>, kind: &str, prev: &str, cur: &str) {
    let p = if prev.trim().is_empty() { "-" } else { prev };
    let c = if cur.trim().is_empty() { "-" } else { cur };
    if p != c {
        out.push(RequalTrigger {
            kind: kind.into(),
            previous: p.into(),
            current: c.into(),
        });
    }
}

// silence unused if LifecycleStatus only via as_str
#[allow(dead_code)]
fn _status_label(s: LifecycleStatus) -> &'static str {
    s.as_str()
}
