//! `wvx` — Weavatrix Loom CLI (thin host over the command bus).

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use wvx_command_bus::{
    implementations_list, load_project_path, project_export_rust, project_export_to_dir,
    project_run, project_validate, registry_implementations, registry_inspect, registry_search,
    registry_summary, BusError,
};
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

Registry defaults to ./registry-dev (or $WVX_REGISTRY).
"
    );
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

fn cmd_registry(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage: wvx registry <summary|search|implementations|inspect> ...");
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
        match project_export_to_dir(&project, &dir, check, run_input) {
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


