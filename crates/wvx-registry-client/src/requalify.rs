//! Continuous requalification when an implementation version / evidence changes.
//!
//! Pilot: re-check lifecycle vs multi-fact evidence (overclaim) and record a
//! timestamped note. Does **not** auto-admit. Full vector re-run stays in
//! `wvx-conformance` / CLI.

use crate::admission::{check_implementation, ImplementationAdmission};
use crate::{LocalRegistry, RegistryError};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use wvx_ir::{AxisFact, EvidenceRecord, LifecycleStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequalifyReport {
    pub implementation_id: String,
    pub ok: bool,
    pub previous_status: String,
    pub justified_status: String,
    pub overclaim: bool,
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
        "Continuous requalification: re-run after bumping implementation version or evidence."
            .into(),
    );
    if admission.overclaim {
        notes.push("OVERCLAIM: declared lifecycle not justified by evidence axes.".into());
    }
    if imp.evidence.conformance == AxisFact::Pass {
        notes.push("conformance axis pass — still not production admission".into());
    }

    Ok(RequalifyReport {
        implementation_id: imp.full_id(),
        ok: !admission.overclaim,
        previous_status: imp.status.as_str().into(),
        justified_status: admission.justified.clone(),
        overclaim: admission.overclaim,
        evidence,
        notes,
        recorded_at_unix: now,
    })
}

// silence unused if LifecycleStatus only via as_str
#[allow(dead_code)]
fn _status_label(s: LifecycleStatus) -> &'static str {
    s.as_str()
}
