//! Cortex: intent → GraphPatch (ops only).
//!
//! ADR-0004 / ADR-0010: AI may **propose** patches; only validated apply is
//! authoritative. Suggestions are never evidence.
//!
//! Sources (in order):
//! 1. **Deterministic heuristics** for known pilot intents (works offline / CI).
//! 2. **xAI LLM** (`XAI_API_KEY`) when heuristics do not match — response must be
//!    a JSON GraphPatch; free-form code is rejected.

use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;
use wvx_ir::{Capability, Project};
use wvx_project_graph::{
    apply_graph_patch, propose_json_pipeline_patch_relative, GraphOp, GraphPatch,
};
use std::collections::BTreeMap;
use std::env;

#[derive(Debug, Error)]
pub enum CortexError {
    #[error("{0}")]
    Msg(String),
    #[error("llm: {0}")]
    Llm(String),
    #[error("invalid GraphPatch from model: {0}")]
    InvalidPatch(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposeSource {
    /// Known intent mapped without calling an LLM.
    Heuristic,
    /// OpenAI-compatible chat completion (xAI).
    Llm,
    /// Relative pilot recipe (same as rule propose).
    RulePilot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentProposeResult {
    pub patch: GraphPatch,
    pub source: ProposeSource,
    /// Whether a dry-run apply succeeded (patch may still be returned if false).
    pub dry_run_ok: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dry_run_errors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Propose a GraphPatch from natural-language intent.
///
/// `capabilities` should be registry contracts when available.
pub fn propose_from_intent(
    intent: &str,
    project: &Project,
    capabilities: &[Capability],
) -> Result<IntentProposeResult, CortexError> {
    let intent = intent.trim();
    if intent.is_empty() {
        return Err(CortexError::Msg("intent is empty".into()));
    }

    let mut result = if let Some(patch) = heuristic_propose(intent, project, capabilities) {
        IntentProposeResult {
            patch,
            source: ProposeSource::Heuristic,
            dry_run_ok: true,
            dry_run_errors: vec![],
            model: None,
        }
    } else if env::var("XAI_API_KEY").is_ok() {
        let model = env::var("WVX_LLM_MODEL").unwrap_or_else(|_| "grok-4-1-fast-non-reasoning".into());
        let patch = llm_propose(intent, project, capabilities, &model)?;
        IntentProposeResult {
            patch,
            source: ProposeSource::Llm,
            dry_run_ok: true,
            dry_run_errors: vec![],
            model: Some(model),
        }
    } else {
        // Offline fallback: if intent looks like "build/install pilot", use rule path.
        let lower = intent.to_ascii_lowercase();
        if looks_like_pilot_intent(&lower) {
            let patch = propose_json_pipeline_patch_relative(project, capabilities);
            IntentProposeResult {
                patch,
                source: ProposeSource::RulePilot,
                dry_run_ok: true,
                dry_run_errors: vec![],
                model: None,
            }
        } else {
            return Err(CortexError::Msg(
                "no heuristic match and XAI_API_KEY is not set. \
                 Set XAI_API_KEY for LLM propose, or try: \"install pilot pipeline\", \
                 \"use pretty serialize\", \"use json-crate parse\"."
                    .into(),
            ));
        }
    };

    // Dry-run apply — never auto-apply (ADR-0004).
    match apply_graph_patch(project, &result.patch) {
        Ok(applied) => {
            if applied.validation.is_ok() {
                result.dry_run_ok = true;
            } else {
                result.dry_run_ok = false;
                result.dry_run_errors = applied
                    .validation
                    .errors()
                    .map(|d| d.message.clone())
                    .collect();
                result.patch.unresolved.push(format!(
                    "dry-run validation failed: {}",
                    result.dry_run_errors.join("; ")
                ));
            }
        }
        Err(e) => {
            result.dry_run_ok = false;
            result.dry_run_errors = vec![e.to_string()];
            result
                .patch
                .unresolved
                .push(format!("dry-run apply failed: {e}"));
        }
    }

    Ok(result)
}

fn looks_like_pilot_intent(lower: &str) -> bool {
    (lower.contains("pilot") || lower.contains("json pipeline") || lower.contains("json-pipeline"))
        && (lower.contains("install")
            || lower.contains("build")
            || lower.contains("create")
            || lower.contains("add")
            || lower.contains("propose")
            || lower.contains("load")
            || lower.contains("full"))
        || lower == "pilot"
        || lower == "json pilot"
        || lower.contains("complete the pilot")
        || lower.contains("finish the pipeline")
}

/// Deterministic intents for CI and offline demos.
fn heuristic_propose(
    intent: &str,
    project: &Project,
    capabilities: &[Capability],
) -> Option<GraphPatch> {
    let lower = intent.to_ascii_lowercase();

    if looks_like_pilot_intent(&lower) {
        let mut patch = propose_json_pipeline_patch_relative(project, capabilities);
        patch.rationale = format!("Heuristic: pilot pipeline for intent `{intent}`");
        return Some(patch);
    }

    // select serialize pretty
    if (lower.contains("pretty") && lower.contains("serial"))
        || lower.contains("pretty print")
        || lower.contains("pretty-print")
        || lower.contains("use pretty serialize")
    {
        return Some(select_impl_patch(
            project,
            "serialize",
            "data.json.serialize",
            "wvx.reference.json-serialize-pretty@1",
            intent,
        ));
    }

    // select compact serialize
    if lower.contains("compact") && lower.contains("serial") {
        return Some(select_impl_patch(
            project,
            "serialize",
            "data.json.serialize",
            "serde-json.serialize@1",
            intent,
        ));
    }

    // select parse backends
    if lower.contains("json-crate") || lower.contains("json crate") {
        return Some(select_impl_patch(
            project,
            "parse",
            "data.json.parse",
            "json-crate.parse@1",
            intent,
        ));
    }
    if lower.contains("reference") && lower.contains("parse") {
        return Some(select_impl_patch(
            project,
            "parse",
            "data.json.parse",
            "wvx.reference.json-parse@1",
            intent,
        ));
    }
    if lower.contains("serde") && lower.contains("parse") {
        return Some(select_impl_patch(
            project,
            "parse",
            "data.json.parse",
            "serde-json.parse-owned@1",
            intent,
        ));
    }

    // add path_set tag loom if missing
    if lower.contains("tag") && (lower.contains("loom") || lower.contains("path")) {
        if project.instance("path_set").is_some() {
            let mut config = BTreeMap::new();
            config.insert("path".into(), json!("/tag"));
            config.insert("value".into(), json!("loom"));
            return Some(GraphPatch {
                ops: vec![GraphOp::SetConfig {
                    instance_id: "path_set".into(),
                    config,
                }],
                rationale: format!("Heuristic: set path_set tag for intent `{intent}`"),
                unresolved: vec![],
            });
        }
        // else fall through to pilot relative
        let mut patch = propose_json_pipeline_patch_relative(project, capabilities);
        patch.rationale =
            format!("Heuristic: ensure pilot path_set (tag) for intent `{intent}`");
        return Some(patch);
    }

    None
}

fn select_impl_patch(
    project: &Project,
    preferred_id: &str,
    cap_id: &str,
    impl_id: &str,
    intent: &str,
) -> GraphPatch {
    let instance_id = project
        .instances
        .iter()
        .find(|i| i.id == preferred_id || i.capability.id == cap_id)
        .map(|i| i.id.clone());

    match instance_id {
        Some(id) => GraphPatch {
            ops: vec![GraphOp::SelectImplementation {
                instance_id: id,
                implementation: Some(impl_id.into()),
            }],
            rationale: format!("Heuristic: select `{impl_id}` for intent `{intent}`"),
            unresolved: vec![],
        },
        None => GraphPatch {
            ops: vec![],
            rationale: format!(
                "Heuristic: wanted `{impl_id}` but no instance with capability `{cap_id}` (or id `{preferred_id}`) exists"
            ),
            unresolved: vec![format!(
                "missing instance for {cap_id}; install pilot pipeline first"
            )],
        },
    }
}

fn llm_propose(
    intent: &str,
    project: &Project,
    capabilities: &[Capability],
    model: &str,
) -> Result<GraphPatch, CortexError> {
    let api_key = env::var("XAI_API_KEY").map_err(|_| {
        CortexError::Llm("XAI_API_KEY not set".into())
    })?;
    let base = env::var("XAI_BASE_URL").unwrap_or_else(|_| "https://api.x.ai/v1".into());

    let cap_summary: Vec<_> = capabilities
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "version": c.version,
                "kind": c.kind,
                "inputs": c.inputs.iter().map(|p| format!("{}:{}", p.id, p.ty)).collect::<Vec<_>>(),
                "outputs": c.outputs.iter().map(|p| format!("{}:{}", p.id, p.ty)).collect::<Vec<_>>(),
            })
        })
        .collect();

    // Compact project view for the model
    let project_view = json!({
        "id": project.id,
        "name": project.name,
        "entrypoint": project.entrypoint,
        "instances": project.instances.iter().map(|i| json!({
            "id": i.id,
            "capability": format!("{}@{}", i.capability.id, i.capability.version),
            "implementation": i.implementation,
            "config": i.config,
        })).collect::<Vec<_>>(),
        "bindings": project.bindings.iter().map(|b| json!({
            "from": format!("{}.{}", b.from.instance, b.from.port),
            "to": format!("{}.{}", b.to.instance, b.to.port),
        })).collect::<Vec<_>>(),
    });

    let system = r#"You are Cortex for Weavatrix Loom.
You ONLY output a single JSON object: a GraphPatch (ops only). No markdown, no prose.

Schema:
{
  "ops": [ GraphOp, ... ],
  "rationale": "string",
  "unresolved": ["string"]
}

GraphOp is tagged with "op" (snake_case):
- {"op":"add_instance","instance":{"id","capability":{"id","version"},"implementation"?:string|null,"config"?:object,"ui"?:{"x","y"}},"capability"?: full capability contract}
- {"op":"remove_instance","id":"..."}
- {"op":"connect","from":{"instance","port"},"to":{"instance","port"}}
- {"op":"disconnect","from":{"instance","port"},"to":{"instance","port"}}
- {"op":"select_implementation","instance_id":"...","implementation":"id@version"|null}
- {"op":"set_config","instance_id":"...","config":{...}}
- {"op":"set_entrypoint","instance_id":"..."|null}

Rules:
- Prefer capabilities from the provided registry list.
- Do not invent free-form Rust code.
- Prefer relative edits against the current project (do not re-add existing instance ids).
- For the JSON pilot, instance ids input/parse/path_set/serialize/output are conventional.
- If unsure, return ops:[] and explain in unresolved.
"#;

    let user = json!({
        "intent": intent,
        "project": project_view,
        "registry_capabilities": cap_summary,
    })
    .to_string();

    let body = json!({
        "model": model,
        "temperature": 0.2,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .map_err(|e| CortexError::Llm(e.to_string()))?;

    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .map_err(|e| CortexError::Llm(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(CortexError::Llm(format!("HTTP {status}: {text}")));
    }

    let v: serde_json::Value = resp
        .json()
        .map_err(|e| CortexError::Llm(e.to_string()))?;
    let content = v
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .ok_or_else(|| CortexError::Llm(format!("unexpected response shape: {v}")))?;

    parse_patch_json(content)
}

fn parse_patch_json(content: &str) -> Result<GraphPatch, CortexError> {
    let trimmed = content.trim();
    // Strip ```json fences if the model ignored instructions.
    let stripped = if let Some(rest) = trimmed.strip_prefix("```") {
        let rest = rest
            .trim_start_matches("json")
            .trim_start_matches("JSON")
            .trim_start();
        rest.strip_suffix("```").unwrap_or(rest).trim()
    } else {
        trimmed
    };

    // If there's prose around JSON, try first { ... last }
    let candidate = if stripped.starts_with('{') {
        stripped.to_string()
    } else if let (Some(a), Some(b)) = (stripped.find('{'), stripped.rfind('}')) {
        stripped[a..=b].to_string()
    } else {
        return Err(CortexError::InvalidPatch(
            "model response did not contain a JSON object".into(),
        ));
    };

    serde_json::from_str::<GraphPatch>(&candidate)
        .map_err(|e| CortexError::InvalidPatch(format!("{e}; body={candidate}")))
}

/// Helpers for tests / demos: build a minimal empty project with schema version.
pub fn empty_project(id: &str, name: &str) -> Project {
    let mut p = Project::new(id, name);
    p.schema_version = wvx_ir::PROJECT_SCHEMA_VERSION.into();
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use wvx_ir::PROJECT_SCHEMA_VERSION;

    #[test]
    fn heuristic_pilot_on_empty() {
        let mut p = Project::new("e", "E");
        p.schema_version = PROJECT_SCHEMA_VERSION.into();
        let r = propose_from_intent("install the pilot json pipeline", &p, &[]).unwrap();
        assert_eq!(r.source, ProposeSource::Heuristic);
        assert!(r.dry_run_ok, "{:?}", r.dry_run_errors);
        assert!(!r.patch.ops.is_empty());
        assert!(r.patch.ops.iter().any(|o| matches!(o, GraphOp::AddInstance { .. })));
    }

    #[test]
    fn heuristic_pretty_serialize() {
        let mut p = Project::new("e", "E");
        p.schema_version = PROJECT_SCHEMA_VERSION.into();
        // install pilot first via heuristic
        let full = propose_from_intent("build pilot", &p, &[]).unwrap();
        let applied = apply_graph_patch(&p, &full.patch).unwrap().project;
        let r = propose_from_intent("use pretty serialize", &applied, &[]).unwrap();
        assert_eq!(r.source, ProposeSource::Heuristic);
        assert!(r.dry_run_ok, "{:?}", r.dry_run_errors);
        assert!(matches!(
            &r.patch.ops[0],
            GraphOp::SelectImplementation {
                instance_id,
                implementation: Some(impl_id),
            } if instance_id == "serialize"
                && impl_id == "wvx.reference.json-serialize-pretty@1"
        ));
    }

    #[test]
    fn heuristic_json_crate_parse() {
        let mut p = Project::new("e", "E");
        p.schema_version = PROJECT_SCHEMA_VERSION.into();
        let full = propose_from_intent("pilot", &p, &[]).unwrap();
        let applied = apply_graph_patch(&p, &full.patch).unwrap().project;
        let r = propose_from_intent("switch parse to json-crate", &applied, &[]).unwrap();
        assert!(r.dry_run_ok, "{:?}", r.dry_run_errors);
        assert!(matches!(
            &r.patch.ops[0],
            GraphOp::SelectImplementation {
                implementation: Some(impl_id),
                ..
            } if impl_id == "json-crate.parse@1"
        ));
    }

    #[test]
    fn empty_intent_errors() {
        let p = empty_project("e", "E");
        assert!(propose_from_intent("  ", &p, &[]).is_err());
    }

    #[test]
    fn parse_patch_from_fenced_json() {
        let raw = r#"```json
{"ops":[{"op":"set_entrypoint","instance_id":"input"}],"rationale":"x","unresolved":[]}
```"#;
        let patch = parse_patch_json(raw).unwrap();
        assert_eq!(patch.ops.len(), 1);
    }
}
