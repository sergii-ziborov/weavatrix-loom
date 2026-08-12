//! FORGE-007: map extract candidates onto the **existing** capability ontology.
//!
//! Order (roadmap §11.7):
//! exact shape → compatible shape → family hint → new capability proposal.
//!
//! Matching is static heuristics only. Never sets evidence pass or admits.

use crate::extract::{ApiCandidate, CandidateShape};
use serde::{Deserialize, Serialize};

/// Lightweight capability view for matching (no dependency on wvx-ir).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyPort {
    pub id: String,
    /// Boundary type string (`bytes`, `json.value` / `json_value`, …).
    pub ty: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyCapability {
    pub id: String,
    pub version: String,
    pub kind: String,
    pub inputs: Vec<OntologyPort>,
    pub outputs: Vec<OntologyPort>,
}

impl OntologyCapability {
    pub fn full_id(&self) -> String {
        format!("{}@{}", self.id, self.version)
    }
}

/// How a candidate maps onto ontology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingKind {
    /// Port types (and usually ids) match an existing capability.
    ExactShape,
    /// Port type multiset matches; ids/order may differ.
    CompatibleShape,
    /// Name/family heuristic only (weak — still a proposal, not auto-wire).
    FamilyHint,
    /// No usable existing match — draft invents a new capability id.
    NewProposal,
}

impl MappingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExactShape => "exact_shape",
            Self::CompatibleShape => "compatible_shape",
            Self::FamilyHint => "family_hint",
            Self::NewProposal => "new_proposal",
        }
    }

    /// True when draft should reuse the existing capability contract id.
    pub fn reuses_existing(self) -> bool {
        matches!(self, Self::ExactShape | Self::CompatibleShape)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityMatch {
    pub kind: MappingKind,
    /// Existing capability full id when reusing, else proposed `…@1`.
    pub capability_key: String,
    pub capability_id: String,
    pub capability_version: String,
    pub score: u32,
    pub rationale: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchReport {
    pub package_name: String,
    pub ontology_size: usize,
    pub matches: Vec<CandidateMatchRow>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateMatchRow {
    pub candidate_name: String,
    pub source_path: String,
    pub source_line: usize,
    pub signature: String,
    pub mapping: CapabilityMatch,
}

/// Match one candidate against the ontology (best score wins).
pub fn match_candidate(
    candidate_name: &str,
    signature: &str,
    shape: &CandidateShape,
    ontology: &[OntologyCapability],
) -> CapabilityMatch {
    let cand_in = normalize_type_multiset(&shape.inputs);
    let cand_out = normalize_type_multiset(&shape.outputs);
    let family = family_hint(candidate_name, signature);

    let mut best: Option<(u32, CapabilityMatch)> = None;

    for cap in ontology {
        // Skip pure I/O sinks/sources for transform-shaped candidates.
        if is_io_passthrough(cap) && (!cand_in.is_empty() || !cand_out.is_empty()) {
            if !(cand_in == multiset_from_ports(&cap.inputs)
                && cand_out == multiset_from_ports(&cap.outputs))
            {
                continue;
            }
        }

        let mut score: u32 = 0;
        let mut rationale = Vec::new();
        let cap_in = multiset_from_ports(&cap.inputs);
        let cap_out = multiset_from_ports(&cap.outputs);

        let types_exact = cand_in == cap_in && cand_out == cap_out;
        let types_compatible = type_multisets_compatible(&cand_in, &cap_in)
            && type_multisets_compatible(&cand_out, &cap_out);
        // path_set / config-augmented: fn takes (value, path, set_to) but capability
        // ports are value→value; treat as compatible when family matches.
        let config_augmented = family == Some("path_set")
            && cap.id.contains("path_set")
            && cand_out.iter().any(|t| t == "json.value")
            && cand_in.iter().any(|t| t == "json.value");

        if types_exact {
            score += 100;
            rationale.push("port type multiset exact".into());
        } else if types_compatible || config_augmented {
            score += 70;
            if config_augmented && !types_compatible {
                rationale.push("config-augmented path_set shape (value ports + config args)".into());
            } else {
                rationale.push("port type multiset compatible".into());
            }
        } else {
            // weak partial: shared types
            let shared_in = shared_count(&cand_in, &cap_in);
            let shared_out = shared_count(&cand_out, &cap_out);
            if shared_in + shared_out == 0 {
                // still allow family name bonus alone
            } else {
                score += 15 * (shared_in + shared_out) as u32;
                rationale.push(format!(
                    "partial type overlap in={shared_in} out={shared_out}"
                ));
            }
        }

        // Port id alignment (bytes/value/…)
        let id_hits = port_id_hits(shape, cap);
        if id_hits > 0 {
            score += 10 * id_hits;
            rationale.push(format!("port id alignment hits={id_hits}"));
        }

        // Family / name hints
        if let Some(fam) = family {
            if cap.id.contains(fam) || family_aliases(fam).iter().any(|a| cap.id.contains(a)) {
                score += 40;
                rationale.push(format!("family hint `{fam}` in capability id"));
            }
        }
        // Function name token in capability id
        let name_l = candidate_name.to_ascii_lowercase();
        if !name_l.is_empty() && cap.id.to_ascii_lowercase().contains(&name_l) {
            score += 20;
            rationale.push("candidate name appears in capability id".into());
        }

        if score == 0 {
            continue;
        }

        let kind = if types_exact && score >= 100 {
            MappingKind::ExactShape
        } else if (types_compatible || config_augmented) && score >= 70 {
            MappingKind::CompatibleShape
        } else if score >= 40 {
            MappingKind::FamilyHint
        } else {
            continue;
        };

        // Prefer stronger kinds; then higher score.
        let rank = kind_rank(kind);
        let better = match &best {
            None => true,
            Some((prev_score, prev)) => {
                let pr = kind_rank(prev.kind);
                rank > pr || (rank == pr && score > *prev_score)
            }
        };
        if better {
            best = Some((
                score,
                CapabilityMatch {
                    kind,
                    capability_key: cap.full_id(),
                    capability_id: cap.id.clone(),
                    capability_version: cap.version.clone(),
                    score,
                    rationale,
                },
            ));
        }
    }

    if let Some((_, m)) = best {
        // Family-only without type support stays a soft hint (caller may still
        // invent a new capability id).
        return m;
    }

    // New proposal key (caller may still slug differently).
    let proposed = format!("proposed.{}.{}@1", "fn", slug_simple(candidate_name));
    CapabilityMatch {
        kind: MappingKind::NewProposal,
        capability_key: proposed.clone(),
        capability_id: proposed.trim_end_matches("@1").to_string(),
        capability_version: "1".into(),
        score: 0,
        rationale: vec![
            "no ontology match above threshold".into(),
            "new capability proposal requires human review (ADR-0010)".into(),
        ],
    }
}

/// Match all function candidates in an extract-shaped list.
pub fn match_candidates(
    package_name: &str,
    candidates: &[ApiCandidate],
    ontology: &[OntologyCapability],
) -> MatchReport {
    let mut rows = Vec::new();
    for c in candidates {
        if c.kind != crate::extract::CandidateKind::Function {
            continue;
        }
        let mapping = match_candidate(&c.name, &c.signature, &c.shape, ontology);
        rows.push(CandidateMatchRow {
            candidate_name: c.name.clone(),
            source_path: c.path.clone(),
            source_line: c.line,
            signature: c.signature.clone(),
            mapping,
        });
    }
    rows.sort_by(|a, b| a.candidate_name.cmp(&b.candidate_name));

    let reused = rows
        .iter()
        .filter(|r| r.mapping.kind.reuses_existing())
        .count();
    let proposed = rows
        .iter()
        .filter(|r| r.mapping.kind == MappingKind::NewProposal)
        .count();

    MatchReport {
        package_name: package_name.into(),
        ontology_size: ontology.len(),
        matches: rows,
        notes: vec![
            "Static capability matching only — not admission (ADR-0007).".into(),
            format!("ontology_size={}; reuses_existing={reused}; new_proposal={proposed}", ontology.len()),
            "AI/heuristic mapping never sets evidence pass or creates public capability without review.".into(),
        ],
    }
}

fn kind_rank(k: MappingKind) -> u8 {
    match k {
        MappingKind::ExactShape => 4,
        MappingKind::CompatibleShape => 3,
        MappingKind::FamilyHint => 2,
        MappingKind::NewProposal => 1,
    }
}

fn is_io_passthrough(cap: &OntologyCapability) -> bool {
    cap.id.starts_with("io.")
}

fn family_hint(name: &str, signature: &str) -> Option<&'static str> {
    let blob = format!("{name} {signature}").to_ascii_lowercase();
    if blob.contains("parse") || blob.contains("from_str") || blob.contains("from_slice") {
        return Some("parse");
    }
    if blob.contains("serialize")
        || blob.contains("to_vec")
        || blob.contains("to_string")
        || blob.contains("encode")
    {
        return Some("serialize");
    }
    if blob.contains("path_set")
        || blob.contains("pointer")
        || blob.contains("set_path")
        || (blob.contains("set") && blob.contains("path"))
    {
        return Some("path_set");
    }
    None
}

fn family_aliases(fam: &str) -> &'static [&'static str] {
    match fam {
        "parse" => &["parse", "decode", "from"],
        "serialize" => &["serialize", "encode", "to_bytes"],
        "path_set" => &["path_set", "pointer", "path"],
        _ => &[],
    }
}

fn normalize_type_token(ty: &str) -> String {
    let t = ty.trim().to_ascii_lowercase();
    // Strip Result wrappers left as notes in extract.
    let t = t
        .trim_start_matches("result<")
        .trim_end_matches('>')
        .split(',')
        .next()
        .unwrap_or(&t)
        .trim();
    match t {
        "json_value" | "json.value" | "value" | "serde_json::value" | "serde_json::value::value" => {
            "json.value".into()
        }
        "bytes" | "&[u8]" | "vec<u8>" | "&vec<u8>" => "bytes".into(),
        "string" | "&str" | "&string" => "string".into(),
        "bool" => "bool".into(),
        "unit" | "()" => "unit".into(),
        "i64" | "i32" | "isize" => "i64".into(),
        "u64" | "u32" | "usize" => "u64".into(),
        "f64" | "f32" => "f64".into(),
        other => other.to_string(),
    }
}

fn normalize_type_multiset(types: &[String]) -> Vec<String> {
    let mut v: Vec<String> = types.iter().map(|t| normalize_type_token(t)).collect();
    v.sort();
    v
}

fn multiset_from_ports(ports: &[OntologyPort]) -> Vec<String> {
    let mut v: Vec<String> = ports.iter().map(|p| normalize_type_token(&p.ty)).collect();
    v.sort();
    v
}

fn type_multisets_compatible(a: &[String], b: &[String]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(x, y)| x == y)
}

fn shared_count(a: &[String], b: &[String]) -> usize {
    let mut bb = b.to_vec();
    let mut n = 0;
    for x in a {
        if let Some(i) = bb.iter().position(|y| y == x) {
            bb.remove(i);
            n += 1;
        }
    }
    n
}

fn port_id_hits(shape: &CandidateShape, cap: &OntologyCapability) -> u32 {
    // Heuristic default port ids used by draft for single-arg shapes.
    let mut hits = 0u32;
    let cand_in_ids: Vec<&str> = if shape.inputs.len() == 1 {
        let ty = normalize_type_token(&shape.inputs[0]);
        vec![default_id_for_ty(&ty)]
    } else {
        Vec::new()
    };
    let cand_out_ids: Vec<&str> = if shape.outputs.len() == 1 {
        let ty = normalize_type_token(&shape.outputs[0]);
        vec![default_id_for_ty(&ty)]
    } else {
        Vec::new()
    };
    for p in &cap.inputs {
        if cand_in_ids.contains(&p.id.as_str()) {
            hits += 1;
        }
    }
    for p in &cap.outputs {
        if cand_out_ids.contains(&p.id.as_str()) {
            hits += 1;
        }
    }
    hits
}

fn default_id_for_ty(ty: &str) -> &'static str {
    match ty {
        "bytes" => "bytes",
        "json.value" => "value",
        "string" => "text",
        "bool" => "flag",
        _ => "value",
    }
}

fn slug_simple(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pilot_ontology() -> Vec<OntologyCapability> {
        vec![
            OntologyCapability {
                id: "data.json.parse".into(),
                version: "1".into(),
                kind: "transform".into(),
                inputs: vec![OntologyPort {
                    id: "bytes".into(),
                    ty: "bytes".into(),
                }],
                outputs: vec![OntologyPort {
                    id: "value".into(),
                    ty: "json_value".into(),
                }],
            },
            OntologyCapability {
                id: "data.json.serialize".into(),
                version: "1".into(),
                kind: "transform".into(),
                inputs: vec![OntologyPort {
                    id: "value".into(),
                    ty: "json_value".into(),
                }],
                outputs: vec![OntologyPort {
                    id: "bytes".into(),
                    ty: "bytes".into(),
                }],
            },
            OntologyCapability {
                id: "data.json.path_set".into(),
                version: "1".into(),
                kind: "transform".into(),
                inputs: vec![OntologyPort {
                    id: "value".into(),
                    ty: "json_value".into(),
                }],
                outputs: vec![OntologyPort {
                    id: "value".into(),
                    ty: "json_value".into(),
                }],
            },
        ]
    }

    #[test]
    fn matches_parse_shape_exact() {
        let shape = CandidateShape {
            inputs: vec!["bytes".into()],
            outputs: vec!["json.value".into()],
            notes: vec![],
        };
        let m = match_candidate("parse", "pub fn parse(bytes: &[u8]) -> Result<Value, E>", &shape, &pilot_ontology());
        assert_eq!(m.kind, MappingKind::ExactShape);
        assert_eq!(m.capability_id, "data.json.parse");
        assert!(m.score >= 100);
    }

    #[test]
    fn matches_serialize_shape() {
        let shape = CandidateShape {
            inputs: vec!["json.value".into()],
            outputs: vec!["bytes".into()],
            notes: vec![],
        };
        let m = match_candidate(
            "to_vec",
            "pub fn to_vec(v: &Value) -> Result<Vec<u8>, E>",
            &shape,
            &pilot_ontology(),
        );
        assert!(m.kind.reuses_existing());
        assert_eq!(m.capability_id, "data.json.serialize");
    }

    #[test]
    fn unknown_shape_is_new_proposal() {
        let shape = CandidateShape {
            inputs: vec!["string".into()],
            outputs: vec!["bool".into()],
            notes: vec![],
        };
        let m = match_candidate("is_valid", "pub fn is_valid(s: &str) -> bool", &shape, &pilot_ontology());
        assert_eq!(m.kind, MappingKind::NewProposal);
    }

    #[test]
    fn family_hint_without_shape_is_soft() {
        let shape = CandidateShape {
            inputs: vec!["i64".into()],
            outputs: vec!["i64".into()],
            notes: vec![],
        };
        let m = match_candidate("parse_int", "pub fn parse_int(n: i64) -> i64", &shape, &pilot_ontology());
        // Name says parse but types don't match bytes→json — family or new.
        assert!(matches!(
            m.kind,
            MappingKind::FamilyHint | MappingKind::NewProposal
        ));
        if m.kind == MappingKind::FamilyHint {
            assert!(!m.kind.reuses_existing());
        }
    }
}
