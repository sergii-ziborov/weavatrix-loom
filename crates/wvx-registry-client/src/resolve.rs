//! Explainable implementation resolver (TargetProfile + ResolverPolicy).
//!
//! Selects one implementation for a capability with an auditable explanation.
//! Does **not** auto-admit; never invents evidence.

use wvx_ir::{
    AxisFact, Implementation, LifecycleStatus, ResolveDecision, ResolveRejection, ResolverPolicy,
    TargetProfile,
};

/// Resolve which implementation to use for `capability_key`.
///
/// `impls` should already be filtered to that capability (or will be filtered here).
pub fn resolve_implementation(
    capability_key: &str,
    impls: &[Implementation],
    profile: &TargetProfile,
    policy: &ResolverPolicy,
) -> ResolveDecision {
    let (want_id, want_ver) = split_cap(capability_key);
    let mut explanation = vec![
        format!("resolve capability `{capability_key}`"),
        format!("policy `{}` · profile `{}`", policy.id, profile.id),
    ];
    let mut rejected = Vec::new();
    let mut pool: Vec<&Implementation> = impls
        .iter()
        .filter(|i| i.capability.id == want_id && i.capability.version == want_ver)
        .collect();

    if pool.is_empty() {
        explanation.push("no implementations listed for this capability".into());
        return ResolveDecision {
            capability_key: capability_key.into(),
            policy_id: policy.id.clone(),
            profile_id: profile.id.clone(),
            chosen: None,
            ranked: Vec::new(),
            explanation,
            rejected,
        };
    }

    // Hard policy filters
    pool.retain(|imp| {
        let full = imp.full_id();
        if !policy.allow_candidate
            && matches!(
                imp.status,
                LifecycleStatus::Candidate | LifecycleStatus::InventoryOnly
            )
        {
            rejected.push(ResolveRejection {
                implementation_id: full,
                reason: "policy disallows candidate/inventory_only".into(),
            });
            return false;
        }
        if policy.require_conformance_pass && imp.evidence.conformance == AxisFact::Fail {
            rejected.push(ResolveRejection {
                implementation_id: full,
                reason: "conformance evidence is fail".into(),
            });
            return false;
        }
        if policy.require_build_pass && imp.evidence.build != AxisFact::Pass {
            rejected.push(ResolveRejection {
                implementation_id: full,
                reason: "build evidence is not pass".into(),
            });
            return false;
        }
        if profile.prefer_no_unsafe {
            if let Some(n) = &imp.notes {
                if n.to_ascii_lowercase().contains("unsafe")
                    || n.to_ascii_lowercase().contains("ffi")
                {
                    rejected.push(ResolveRejection {
                        implementation_id: full,
                        reason: "profile prefer_no_unsafe: notes mention unsafe/ffi".into(),
                    });
                    return false;
                }
            }
        }
        true
    });

    // Rank: prefer_impl_ids, then lifecycle, then conformance pass, then pure-rust hint
    pool.sort_by(|a, b| {
        let sa = score(a, profile, policy);
        let sb = score(b, profile, policy);
        sb.cmp(&sa).then_with(|| a.full_id().cmp(&b.full_id()))
    });

    let ranked: Vec<String> = pool.iter().map(|i| i.full_id()).collect();
    explanation.push(format!("{} candidate(s) after filters", ranked.len()));
    for (i, id) in ranked.iter().enumerate() {
        explanation.push(format!("  rank {}: {id}", i + 1));
    }

    let chosen = ranked.first().cloned();
    if let Some(ref c) = chosen {
        explanation.push(format!("chosen `{c}` (highest rank after policy+profile)"));
        if let Some(pref) = policy
            .prefer_impl_ids
            .iter()
            .find(|p| ranked.iter().any(|r| r == *p))
        {
            if pref == c {
                explanation.push(format!("matches policy prefer_impl_ids entry `{pref}`"));
            }
        }
    } else {
        explanation.push("no implementation survived policy filters".into());
    }

    ResolveDecision {
        capability_key: capability_key.into(),
        policy_id: policy.id.clone(),
        profile_id: profile.id.clone(),
        chosen,
        ranked,
        explanation,
        rejected,
    }
}

fn score(imp: &Implementation, profile: &TargetProfile, policy: &ResolverPolicy) -> i32 {
    let mut s = 0i32;
    if let Some(pos) = policy
        .prefer_impl_ids
        .iter()
        .position(|p| p == &imp.full_id())
    {
        s += 10_000 - (pos as i32 * 10);
    }
    s += match imp.status {
        LifecycleStatus::Admitted => 400,
        LifecycleStatus::Conformant => 300,
        LifecycleStatus::Candidate => 100,
        LifecycleStatus::InventoryOnly => 10,
    };
    if imp.evidence.conformance == AxisFact::Pass {
        s += 50;
    }
    if imp.evidence.build == AxisFact::Pass {
        s += 20;
    }
    if profile.prefer_pure_rust {
        let pkg = imp.source.package.as_str();
        // Heuristic: known pure-Rust pilot packages
        if matches!(
            pkg,
            "sha2" | "blake3" | "flate2" | "serde_json" | "json" | "wvx-adapters" | "wvx-adapter-external-demo"
        ) {
            s += 15;
        }
    }
    s
}

fn split_cap(key: &str) -> (String, String) {
    if let Some((id, ver)) = key.rsplit_once('@') {
        (id.to_string(), ver.to_string())
    } else {
        (key.to_string(), "1".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::{CapabilityRef, ImplementationEvidence, ImplementationSource};

    fn imp(id: &str, status: LifecycleStatus, conf: AxisFact) -> Implementation {
        Implementation {
            id: id.into(),
            version: "1".into(),
            capability: CapabilityRef::new("data.hash.sha256", "1"),
            source: ImplementationSource {
                kind: "crates-io".into(),
                package: "sha2".into(),
                package_version: "0.10".into(),
                notes: None,
            },
            adapter: None,
            status,
            evidence: ImplementationEvidence {
                build: AxisFact::Pass,
                conformance: conf,
                ..Default::default()
            },
            notes: None,
            sdk: None,
        conformance_profile: None,
        evidence_artifact: None,
        }
    }

    #[test]
    fn prefers_conformant_and_explains() {
        let impls = vec![
            imp("sha2.sha256-streaming", LifecycleStatus::Candidate, AxisFact::Pass),
            imp("sha2.sha256", LifecycleStatus::Conformant, AxisFact::Pass),
        ];
        let d = resolve_implementation(
            "data.hash.sha256@1",
            &impls,
            &TargetProfile {
                id: "dev".into(),
                prefer_pure_rust: true,
                ..Default::default()
            },
            &ResolverPolicy::default(),
        );
        assert_eq!(d.chosen.as_deref(), Some("sha2.sha256@1"));
        assert!(d.explanation.iter().any(|e| e.contains("chosen")));
    }
}
