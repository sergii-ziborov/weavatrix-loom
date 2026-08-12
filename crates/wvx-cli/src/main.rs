//! `wvx` — Weavatrix Loom CLI (thin host over the command bus).

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use wvx_command_bus::{
    forge_draft, forge_extract, forge_inventory, forge_match, graph_apply_patch,
    graph_propose_intent, graph_propose_patch, implementations_list, load_project_path, pilot_bench,
    project_export_rust, project_export_to_dir_with_registry, project_run, project_validate,
    registry_admission_audit, registry_human_admit, registry_implementations, registry_inspect,
    registry_search, registry_summary, BusError,
};
use wvx_registry_client::AdmitRequest;
use wvx_project_graph::GraphPatch;
use wvx_registry_client::LocalRegistry;
use wvx_types::WvxValue;

fn main() -> ExitCode {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return ExitCode::SUCCESS;
    }

    let cmd = args.remove(0);
    match cmd.as_str() {
        "validate" => cmd_validate(&args),
        "run" => cmd_run(&args),
        "implementations" | "impls" => cmd_implementations(),
        "export-rust" => cmd_export(&args),
        "registry" => cmd_registry(&args),
        "forge" => cmd_forge(&args),
        "patch" => cmd_patch(&args),
        "conformance" => cmd_conformance(&args),
        "bench" => cmd_bench(&args),
        "registry-search" => {
            // Back-compat alias: registry search [query]
            let mut rest = vec!["search".into()];
            rest.extend(args);
            cmd_registry(&rest)
        }
        "version" => {
            println!("wvx {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command: {other}");
            print_help();
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!(
        "\
Weavatrix Loom CLI

Usage:
  wvx validate <project.wvx.json>
  wvx run <project.wvx.json> [options]
  wvx implementations
  wvx export-rust <project.wvx.json> [-o <dir>] [--check] [--run] [--impl id=impl]...
  wvx registry summary [--path <dir>]
  wvx registry search [query] [--path <dir>]
  wvx registry implementations [--capability key] [query] [--path <dir>]
  wvx registry inspect <key> [--path <dir>]
  wvx registry check|audit [--path <dir>]   lifecycle vs evidence (overclaim fail)
  wvx registry admit <impl-id> --reviewer <name> --human-ack <text> --security-ack <text> \\
      --reason <text> --bench-file <path> [--apply] [--path <dir>]
  wvx forge inventory <crate-or-workspace-path>
  wvx forge extract <crate-path>
  wvx forge draft <crate-path> [--name <substr>] [-o <dir>]   static adapter drafts
  wvx patch propose [project.wvx.json]   relative if project given; full pilot if omitted
  wvx patch intent <text> [--project <file>]   heuristic or LLM (XAI_API_KEY) → GraphPatch
  wvx patch apply <project.wvx.json> [--patch <patch.json>]
  wvx conformance [--golden]
  wvx bench [--iterations N] [--warmup N] [-o file.json]   Gate E pilot microbench
  wvx version

Run options:
  --input <file|->              raw input bytes (default: {{\"hello\":\"world\"}})
  --input-json <json>           JSON text as input bytes
  --impl <instance>=<impl-id>   swap implementation without changing the graph
                                (repeatable; e.g. --impl parse=wvx.reference.json-parse@1)

Export options:
  -o, --out <dir>               write Cargo package to directory
  --check                       run cargo check after write
  --run                         cargo run after check (uses same input as run)

`run` uses the playground. `export-rust` emits a native Rust package whose
`run_pipeline` should match playground results for the pilot adapters.

  wvx conformance               pilot vectors + negative parse/path_set error codes
  wvx conformance --golden      also dynamic≡static export combos (invokes cargo)
  wvx bench                     Gate E pilot microbench (benchmark evidence axis)
  wvx registry admit            Gate E human admit (fail-closed; --apply writes manifest)

Registry defaults to ./registry-dev (or $WVX_REGISTRY).
"
    );
}

fn cmd_bench(args: &[String]) -> ExitCode {
    let mut iterations = 200u32;
    let mut warmup = 20u32;
    let mut out: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--iterations" || args[i] == "-n" {
            if let Some(v) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                iterations = v;
            }
            i += 2;
            continue;
        }
        if args[i] == "--warmup" {
            if let Some(v) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                warmup = v;
            }
            i += 2;
            continue;
        }
        if args[i] == "-o" || args[i] == "--out" {
            out = args.get(i + 1).map(PathBuf::from);
            i += 2;
            continue;
        }
        i += 1;
    }
    let resp = pilot_bench(iterations, warmup);
    let text = serde_json::to_string_pretty(&resp).unwrap();
    if let Some(path) = &out {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(e) = fs::write(path, &text) {
            eprintln!("write {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
        eprintln!("wrote {}", path.display());
    }
    println!("{text}");
    if resp.data.as_ref().map(|d| d.ok).unwrap_or(false) {
        eprintln!(
            "bench: PASS ({} cases)",
            resp.data.as_ref().map(|d| d.cases.len()).unwrap_or(0)
        );
        ExitCode::SUCCESS
    } else {
        eprintln!("bench: FAIL");
        ExitCode::FAILURE
    }
}

fn cmd_conformance(args: &[String]) -> ExitCode {
    let golden = args.iter().any(|a| a == "--golden");
    let report = wvx_conformance::run_pilot_conformance();
    let failed = report.cases.iter().filter(|c| !c.ok).count();
    println!(
        "conformance: {} cases, {} failed",
        report.cases.len(),
        failed
    );
    for c in &report.cases {
        let mark = if c.ok { "ok" } else { "FAIL" };
        println!(
            "  [{mark}] {} / {} / {}",
            c.capability, c.implementation, c.case
        );
        if let Some(d) = &c.detail {
            println!("         {d}");
        }
    }
    if !report.ok {
        return ExitCode::FAILURE;
    }

    if golden {
        println!("golden dynamic≡static (compact combos)…");
        let goldens = wvx_conformance::run_all_goldens(br#"{"hello":"world"}"#);
        let mut all_ok = true;
        for g in &goldens {
            let mark = if g.ok { "ok" } else { "FAIL" };
            println!(
                "  [{mark}] parse={} serialize={}",
                g.parse_impl, g.serialize_impl
            );
            if let Some(d) = &g.detail {
                println!("         {d}");
            }
            if g.ok {
                println!("         json={}", g.dynamic_json);
            }
            all_ok &= g.ok;
        }
        if !all_ok {
            return ExitCode::FAILURE;
        }
        println!("golden: all combos passed");
    }

    println!("conformance: PASS");
    ExitCode::SUCCESS
}

fn open_registry(args: &[String]) -> Result<LocalRegistry, String> {
    let mut path: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--path" {
            let p = args
                .get(i + 1)
                .ok_or_else(|| "--path requires a directory".to_string())?;
            path = Some(PathBuf::from(p));
            i += 2;
            continue;
        }
        i += 1;
    }
    match path {
        Some(p) => LocalRegistry::open(p).map_err(|e| e.to_string()),
        None => LocalRegistry::open_default().map_err(|e| e.to_string()),
    }
}

fn args_without_path(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--path" {
            i += 2;
            continue;
        }
        out.push(args[i].clone());
        i += 1;
    }
    out
}

fn cmd_forge(args: &[String]) -> ExitCode {
    if args.is_empty() || args[0] == "help" {
        eprintln!(
            "usage: wvx forge <inventory|extract|match|draft> <path> [options]\n\
             draft/match use $WVX_REGISTRY / registry-dev for capability ontology (FORGE-007)"
        );
        return ExitCode::FAILURE;
    }
    match args[0].as_str() {
        "inventory" => {
            let Some(path) = args.get(1) else {
                eprintln!("usage: wvx forge inventory <path>");
                return ExitCode::FAILURE;
            };
            match forge_inventory(path.as_ref()) {
                Ok(resp) => {
                    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        "extract" => {
            let Some(path) = args.get(1) else {
                eprintln!("usage: wvx forge extract <crate-path>");
                return ExitCode::FAILURE;
            };
            match forge_extract(path.as_ref()) {
                Ok(resp) => {
                    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        "match" => {
            // wvx forge match <path>
            let Some(path) = args.get(1) else {
                eprintln!("usage: wvx forge match <crate-path>");
                return ExitCode::FAILURE;
            };
            let reg = LocalRegistry::open_default().ok();
            match forge_match(path.as_ref(), reg.as_ref()) {
                Ok(resp) => {
                    if let Some(r) = &resp.data {
                        let reused = r
                            .matches
                            .iter()
                            .filter(|m| {
                                m.mapping.kind == wvx_forge::MappingKind::ExactShape
                                    || m.mapping.kind == wvx_forge::MappingKind::CompatibleShape
                            })
                            .count();
                        eprintln!(
                            "forge match: {} candidate(s) · ontology={} · reuses={}",
                            r.matches.len(),
                            r.ontology_size,
                            reused
                        );
                    }
                    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        "draft" => {
            // wvx forge draft <path> [--name substr] [-o dir]
            let Some(path) = args.get(1) else {
                eprintln!("usage: wvx forge draft <crate-path> [--name <substr>] [-o <dir>]");
                return ExitCode::FAILURE;
            };
            let mut name = None;
            let mut out = None;
            let mut i = 2;
            while i < args.len() {
                if args[i] == "--name" || args[i] == "-n" {
                    name = args.get(i + 1).map(|s| s.as_str());
                    i += 2;
                    continue;
                }
                if args[i] == "-o" || args[i] == "--out" {
                    out = args.get(i + 1).map(|s| s.as_str());
                    i += 2;
                    continue;
                }
                i += 1;
            }
            let reg = LocalRegistry::open_default().ok();
            match forge_draft(
                path.as_ref(),
                name,
                out.map(std::path::Path::new),
                reg.as_ref(),
            ) {
                Ok(resp) => {
                    if let Some(r) = &resp.data {
                        let reused = r
                            .drafts
                            .iter()
                            .filter(|d| {
                                d.mapping_kind == "exact_shape"
                                    || d.mapping_kind == "compatible_shape"
                            })
                            .count();
                        eprintln!(
                            "forge draft: {} · {} draft(s) · {} · ontology_reuse={}",
                            r.status,
                            r.drafts.len(),
                            r.package_name,
                            reused
                        );
                    }
                    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        other => {
            eprintln!("unknown forge subcommand: {other}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_patch(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage: wvx patch <propose|apply> ...");
        return ExitCode::FAILURE;
    }
    match args[0].as_str() {
        "propose" => {
            let reg = LocalRegistry::open_default().ok();
            // Optional: wvx patch propose [project.wvx.json]  → relative propose
            let base = if let Some(path) = args.get(1) {
                match load_project_path(path.as_ref()) {
                    Ok(p) => Some(p),
                    Err(e) => {
                        eprintln!("{e}");
                        return ExitCode::FAILURE;
                    }
                }
            } else {
                None
            };
            match graph_propose_patch(reg.as_ref(), base.as_ref()) {
                Ok(resp) => {
                    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        "intent" => {
            // wvx patch intent "install pilot" [--project file]
            let mut intent_parts = Vec::new();
            let mut project_path: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                if args[i] == "--project" {
                    project_path = args.get(i + 1).cloned();
                    i += 2;
                    continue;
                }
                intent_parts.push(args[i].clone());
                i += 1;
            }
            let intent = intent_parts.join(" ");
            if intent.trim().is_empty() {
                eprintln!("usage: wvx patch intent <text> [--project file.wvx.json]");
                return ExitCode::FAILURE;
            }
            let project = if let Some(p) = project_path {
                match load_project_path(p.as_ref()) {
                    Ok(proj) => proj,
                    Err(e) => {
                        eprintln!("{e}");
                        return ExitCode::FAILURE;
                    }
                }
            } else {
                let mut p = wvx_ir::Project::new("intent", "Intent base");
                p.schema_version = wvx_ir::PROJECT_SCHEMA_VERSION.into();
                p
            };
            let reg = LocalRegistry::open_default().ok();
            match graph_propose_intent(reg.as_ref(), &project, &intent) {
                Ok(resp) => {
                    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
                    if resp.ok {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        "apply" => {
            let Some(project_path) = args.get(1) else {
                eprintln!("usage: wvx patch apply <project.wvx.json> [--patch file]");
                return ExitCode::FAILURE;
            };
            let mut patch: Option<GraphPatch> = None;
            let mut i = 2;
            while i < args.len() {
                if args[i] == "--patch" {
                    let p = match args.get(i + 1) {
                        Some(p) => p,
                        None => {
                            eprintln!("--patch requires a file");
                            return ExitCode::FAILURE;
                        }
                    };
                    let text = match fs::read_to_string(p) {
                        Ok(t) => t,
                        Err(e) => {
                            eprintln!("{e}");
                            return ExitCode::FAILURE;
                        }
                    };
                    patch = match serde_json::from_str(&text) {
                        Ok(p) => Some(p),
                        Err(e) => {
                            eprintln!("invalid patch: {e}");
                            return ExitCode::FAILURE;
                        }
                    };
                    i += 2;
                } else {
                    i += 1;
                }
            }
            let project = match load_project_path(project_path.as_ref()) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            };
            let patch = match patch {
                Some(p) => p,
                None => {
                    match graph_propose_patch(
                        LocalRegistry::open_default().ok().as_ref(),
                        Some(&project),
                    ) {
                        Ok(r) => r.data.expect("patch"),
                        Err(e) => {
                            eprintln!("{e}");
                            return ExitCode::FAILURE;
                        }
                    }
                }
            };
            match graph_apply_patch(&project, &patch) {
                Ok(resp) => {
                    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
                    if resp.ok {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        other => {
            eprintln!("unknown patch subcommand: {other}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_registry(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage: wvx registry <summary|search|implementations|inspect|check> ...");
        return ExitCode::FAILURE;
    }
    let sub = args[0].as_str();
    let rest = &args[1..];
    let reg = match open_registry(rest) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let rest = args_without_path(rest);

    let result = match sub {
        "summary" | "list" => registry_summary(&reg).map(|r| serde_json::to_string_pretty(&r).unwrap()),
        "search" => {
            let q = rest.first().map(String::as_str).unwrap_or("");
            registry_search(&reg, q).map(|r| serde_json::to_string_pretty(&r).unwrap())
        }
        "implementations" | "impls" => {
            let mut capability = None;
            let mut query = String::new();
            let mut i = 0;
            while i < rest.len() {
                if rest[i] == "--capability" || rest[i] == "-c" {
                    capability = rest.get(i + 1).map(|s| s.as_str());
                    i += 2;
                    continue;
                }
                if query.is_empty() {
                    query = rest[i].clone();
                }
                i += 1;
            }
            registry_implementations(&reg, capability, &query)
                .map(|r| serde_json::to_string_pretty(&r).unwrap())
        }
        "inspect" => {
            let Some(key) = rest.first() else {
                eprintln!("usage: wvx registry inspect <capability-or-impl-key>");
                return ExitCode::FAILURE;
            };
            registry_inspect(&reg, key).map(|r| serde_json::to_string_pretty(&r).unwrap())
        }
        "check" | "audit" | "admission" => {
            match registry_admission_audit(&reg) {
                Ok(resp) => {
                    let report = resp.data.as_ref();
                    if let Some(r) = report {
                        eprintln!(
                            "admission audit: {} checked, {} overclaim, {} underclaim — {}",
                            r.checked,
                            r.overclaims,
                            r.underclaims,
                            if r.ok { "PASS" } else { "FAIL" }
                        );
                        for item in &r.items {
                            if item.overclaim || item.underclaim || !item.findings.is_empty() {
                                let mark = if item.overclaim {
                                    "OVER"
                                } else if item.underclaim {
                                    "under"
                                } else {
                                    "info"
                                };
                                eprintln!(
                                    "  [{mark}] {} declared={} justified={}",
                                    item.full_id, item.declared, item.justified
                                );
                                for f in &item.findings {
                                    eprintln!("         {}: {}", f.severity, f.message);
                                }
                            }
                        }
                    }
                    let text = serde_json::to_string_pretty(&resp).unwrap();
                    if resp.data.as_ref().map(|d| d.ok).unwrap_or(false) {
                        Ok(text)
                    } else {
                        println!("{text}");
                        return ExitCode::FAILURE;
                    }
                }
                Err(e) => Err(e),
            }
        }
        "admit" => {
            // wvx registry admit <impl-id> --reviewer … --human-ack … --security-ack … --reason … --bench-file … [--apply]
            let Some(impl_id) = rest.first().cloned() else {
                eprintln!(
                    "usage: wvx registry admit <impl-id> --reviewer <name> --human-ack <text> \\\n  --security-ack <text> --reason <text> --bench-file <path> [--apply]"
                );
                return ExitCode::FAILURE;
            };
            let mut reviewer = String::new();
            let mut human_ack = String::new();
            let mut security_ack = String::new();
            let mut reason = String::new();
            let mut bench_file: Option<PathBuf> = None;
            let mut apply = false;
            let mut i = 1;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--reviewer" => {
                        reviewer = rest.get(i + 1).cloned().unwrap_or_default();
                        i += 2;
                    }
                    "--human-ack" => {
                        human_ack = rest.get(i + 1).cloned().unwrap_or_default();
                        i += 2;
                    }
                    "--security-ack" => {
                        security_ack = rest.get(i + 1).cloned().unwrap_or_default();
                        i += 2;
                    }
                    "--reason" => {
                        reason = rest.get(i + 1).cloned().unwrap_or_default();
                        i += 2;
                    }
                    "--bench-file" => {
                        bench_file = rest.get(i + 1).map(PathBuf::from);
                        i += 2;
                    }
                    "--apply" => {
                        apply = true;
                        i += 1;
                    }
                    _ => i += 1,
                }
            }
            let bench_fingerprint = match bench_file {
                Some(p) => match fs::read_to_string(&p) {
                    Ok(t) => {
                        // Prefer provenance.input_fingerprint from a bench BusResponse
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                            v.pointer("/data/provenance/input_fingerprint")
                                .and_then(|x| x.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| {
                                    v.pointer("/provenance/input_fingerprint")
                                        .and_then(|x| x.as_str())
                                        .map(|s| s.to_string())
                                })
                                .unwrap_or_else(|| {
                                    format!("file:{}:len={}", p.display(), t.len())
                                })
                        } else {
                            format!("file:{}:len={}", p.display(), t.len())
                        }
                    }
                    Err(e) => {
                        eprintln!("bench-file {}: {e}", p.display());
                        return ExitCode::FAILURE;
                    }
                },
                None => {
                    eprintln!("--bench-file is required (run: wvx bench -o .lab/bench.json)");
                    return ExitCode::FAILURE;
                }
            };
            // Require bench report ok if it is a structured report
            if let Some(p) = rest.iter().position(|a| a == "--bench-file") {
                if let Some(bp) = rest.get(p + 1) {
                    if let Ok(t) = fs::read_to_string(bp) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                            let ok = v
                                .pointer("/data/ok")
                                .or_else(|| v.get("ok"))
                                .and_then(|x| x.as_bool())
                                .unwrap_or(true);
                            if !ok {
                                eprintln!("bench-file reports ok=false — refuse admit");
                                return ExitCode::FAILURE;
                            }
                        }
                    }
                }
            }
            let req = AdmitRequest {
                implementation_id: impl_id,
                reviewer,
                human_ack,
                security_ack,
                reason,
                bench_fingerprint,
                apply,
            };
            return match registry_human_admit(&reg, req) {
                Ok(resp) => {
                    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
                    if resp.ok {
                        eprintln!("admit: PASS");
                        ExitCode::SUCCESS
                    } else {
                        eprintln!("admit: FAIL");
                        ExitCode::FAILURE
                    }
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            };
        }
        other => {
            eprintln!("unknown registry subcommand: {other}");
            return ExitCode::FAILURE;
        }
    };

    match result {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(BusError::Registry(e)) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_validate(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else {
        eprintln!("usage: wvx validate <project.wvx.json>");
        return ExitCode::FAILURE;
    };
    match load_project_path(path.as_ref()) {
        Ok(project) => {
            let resp = project_validate(&project);
            println!("{}", serde_json::to_string_pretty(&resp).unwrap());
            if resp.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_implementations() -> ExitCode {
    let resp = implementations_list();
    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
    ExitCode::SUCCESS
}

fn cmd_run(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else {
        eprintln!("usage: wvx run <project.wvx.json> [--impl id=impl] ...");
        return ExitCode::FAILURE;
    };

    let (input_bytes, impl_overrides) = match parse_run_options(&args[1..]) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    match load_project_path(path.as_ref()).and_then(|p| project_run(&p, input_bytes, &impl_overrides))
    {
        Ok(resp) => {
            if env::var_os("WVX_RUN_JSON").is_some() {
                println!("{}", serde_json::to_string_pretty(&resp).unwrap());
            } else if let Some(data) = &resp.data {
                for tr in &data.traces {
                    let status = if tr.error.is_some() { "ERR" } else { "ok" };
                    let impl_s = tr
                        .implementation
                        .as_deref()
                        .unwrap_or("-");
                    println!(
                        "{:>10.3} ms  [{status}]  {}  ({})  impl={}",
                        tr.duration_ms, tr.instance_id, tr.capability, impl_s
                    );
                }
                if let Some(WvxValue::Bytes(b)) = data
                    .outputs
                    .get("output.bytes")
                    .or_else(|| data.outputs.get("serialize.bytes"))
                {
                    match String::from_utf8(b.clone()) {
                        Ok(s) => println!("\noutput:\n{s}"),
                        Err(_) => println!("\noutput: {} bytes (binary)", b.len()),
                    }
                }
            }
            if resp.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn parse_run_options(flags: &[String]) -> Result<(Vec<u8>, BTreeMap<String, String>), String> {
    let mut input: Option<Vec<u8>> = None;
    let mut overrides = BTreeMap::new();
    let mut i = 0;
    while i < flags.len() {
        match flags[i].as_str() {
            "--input" => {
                let path = flags
                    .get(i + 1)
                    .ok_or_else(|| "--input requires a path or -".to_string())?;
                input = Some(if path == "-" {
                    use std::io::Read;
                    let mut buf = Vec::new();
                    std::io::stdin()
                        .read_to_end(&mut buf)
                        .map_err(|e| e.to_string())?;
                    buf
                } else {
                    fs::read(path).map_err(|e| e.to_string())?
                });
                i += 2;
            }
            "--input-json" => {
                let json = flags
                    .get(i + 1)
                    .ok_or_else(|| "--input-json requires a JSON string".to_string())?;
                let _: serde_json::Value =
                    serde_json::from_str(json).map_err(|e| format!("invalid --input-json: {e}"))?;
                input = Some(json.as_bytes().to_vec());
                i += 2;
            }
            "--impl" => {
                let spec = flags
                    .get(i + 1)
                    .ok_or_else(|| "--impl requires instance=implementation-id".to_string())?;
                let (instance, impl_id) = spec
                    .split_once('=')
                    .ok_or_else(|| format!("--impl expected instance=impl-id, got `{spec}`"))?;
                if instance.is_empty() || impl_id.is_empty() {
                    return Err(format!("--impl expected instance=impl-id, got `{spec}`"));
                }
                overrides.insert(instance.to_string(), impl_id.to_string());
                i += 2;
            }
            other => return Err(format!("unknown run option: {other}")),
        }
    }
    Ok((
        input.unwrap_or_else(|| br#"{"hello":"world"}"#.to_vec()),
        overrides,
    ))
}

fn cmd_export(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else {
        eprintln!(
            "usage: wvx export-rust <project.wvx.json> [-o dir] [--check] [--run] [--impl id=impl]"
        );
        return ExitCode::FAILURE;
    };

    let mut out_dir: Option<PathBuf> = None;
    let mut check = false;
    let mut do_run = false;
    let mut overrides = BTreeMap::new();
    let mut input = br#"{"hello":"world"}"#.to_vec();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => {
                let d = args
                    .get(i + 1)
                    .ok_or_else(|| "-o requires a directory".to_string());
                match d {
                    Ok(dir) => {
                        out_dir = Some(PathBuf::from(dir));
                        i += 2;
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        return ExitCode::FAILURE;
                    }
                }
            }
            "--check" => {
                check = true;
                i += 1;
            }
            "--run" => {
                do_run = true;
                check = true;
                i += 1;
            }
            "--impl" => {
                let spec = match args.get(i + 1) {
                    Some(s) => s,
                    None => {
                        eprintln!("--impl requires instance=implementation-id");
                        return ExitCode::FAILURE;
                    }
                };
                let Some((instance, impl_id)) = spec.split_once('=') else {
                    eprintln!("--impl expected instance=impl-id");
                    return ExitCode::FAILURE;
                };
                overrides.insert(instance.to_string(), impl_id.to_string());
                i += 2;
            }
            "--input-json" => {
                let json = match args.get(i + 1) {
                    Some(s) => s,
                    None => {
                        eprintln!("--input-json requires JSON");
                        return ExitCode::FAILURE;
                    }
                };
                if let Err(e) = serde_json::from_str::<serde_json::Value>(json) {
                    eprintln!("invalid --input-json: {e}");
                    return ExitCode::FAILURE;
                }
                input = json.as_bytes().to_vec();
                i += 2;
            }
            other => {
                eprintln!("unknown export option: {other}");
                return ExitCode::FAILURE;
            }
        }
    }

    let mut project = match load_project_path(path.as_ref()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    wvx_runtime::apply_implementation_overrides(&mut project, &overrides);

    if let Some(dir) = out_dir {
        let run_input = if do_run { Some(input.as_slice()) } else { None };
        let reg = LocalRegistry::open_default().ok();
        match project_export_to_dir_with_registry(
            &project,
            &dir,
            check,
            run_input,
            reg.as_ref(),
        ) {
            Ok(resp) => {
                println!("{}", serde_json::to_string_pretty(&resp).unwrap());
                if let Some(data) = &resp.data {
                    if let Some(stdout) = &data.run_stdout {
                        println!("\n--- pipeline stdout ---\n{stdout}");
                    }
                }
                if resp.ok {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        }
    } else {
        match project_export_rust(&project) {
            Ok(resp) => {
                if let Some(ws) = &resp.data {
                    for file in &ws.files {
                        println!("// --- {} ---", file.relative_path);
                        println!("{}", file.contents);
                    }
                }
                if resp.ok {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        }
    }
}


