//! `wvx` — Weavatrix Loom CLI (thin host over the command bus).

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use wvx_command_bus::{
    implementations_list, load_project_path, project_export_rust, project_run, project_validate,
    registry_search, BusError,
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
        "registry-search" => cmd_registry_search(&args),
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
  wvx export-rust <project.wvx.json>
  wvx registry-search <registry-dir> [query]
  wvx version

Run options:
  --input <file|->              raw input bytes (default: {{\"hello\":\"world\"}})
  --input-json <json>           JSON text as input bytes
  --impl <instance>=<impl-id>   swap implementation without changing the graph
                                (repeatable; e.g. --impl parse=wvx.reference.json-parse@1)

`run` uses built-in pilot playground handlers. Trace lines show which
implementation executed for each instance.
"
    );
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
        eprintln!("usage: wvx export-rust <project.wvx.json>");
        return ExitCode::FAILURE;
    };
    match load_project_path(path.as_ref()).and_then(|p| project_export_rust(&p)) {
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

fn cmd_registry_search(args: &[String]) -> ExitCode {
    let Some(root) = args.first() else {
        eprintln!("usage: wvx registry-search <registry-dir> [query]");
        return ExitCode::FAILURE;
    };
    let query = args.get(1).map(String::as_str).unwrap_or("");
    let root = PathBuf::from(root);
    let reg = match LocalRegistry::open(&root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    match registry_search(&reg, query) {
        Ok(resp) => {
            println!("{}", serde_json::to_string_pretty(&resp).unwrap());
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
