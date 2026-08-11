//! `wvx` — Weavatrix Loom CLI (thin host over the command bus).

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use wvx_command_bus::{
    load_project_path, project_export_rust, project_run, project_validate, registry_search,
    BusError,
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
  wvx run <project.wvx.json> [--input <file|->] [--input-json <json>]
  wvx export-rust <project.wvx.json>
  wvx registry-search <registry-dir> [query]
  wvx version

`run` uses built-in pilot playground handlers (JSON pipeline).
Default input is {{\"hello\":\"world\"}} when neither --input nor --input-json is set.
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

fn cmd_run(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else {
        eprintln!("usage: wvx run <project.wvx.json> [--input <file|->] [--input-json <json>]");
        return ExitCode::FAILURE;
    };

    let input_bytes = match resolve_input(args) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    match load_project_path(path.as_ref()).and_then(|p| project_run(&p, input_bytes)) {
        Ok(resp) => {
            // Pretty-print a compact summary for humans; full JSON when WVX_RUN_JSON=1.
            if env::var_os("WVX_RUN_JSON").is_some() {
                println!("{}", serde_json::to_string_pretty(&resp).unwrap());
            } else if let Some(data) = &resp.data {
                for tr in &data.traces {
                    let status = if tr.error.is_some() { "ERR" } else { "ok" };
                    println!(
                        "{:>10.3} ms  [{status}]  {}  ({})",
                        tr.duration_ms, tr.instance_id, tr.capability
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

fn resolve_input(args: &[String]) -> Result<Vec<u8>, String> {
    // args[0] is project path; flags follow as pairs.
    let flags = &args[1..];
    if flags.is_empty() {
        return Ok(br#"{"hello":"world"}"#.to_vec());
    }
    match flags[0].as_str() {
        "--input" => {
            let path = flags
                .get(1)
                .ok_or_else(|| "--input requires a path or -".to_string())?;
            if path == "-" {
                use std::io::Read;
                let mut buf = Vec::new();
                std::io::stdin()
                    .read_to_end(&mut buf)
                    .map_err(|e| e.to_string())?;
                return Ok(buf);
            }
            fs::read(path).map_err(|e| e.to_string())
        }
        "--input-json" => {
            let json = flags
                .get(1)
                .ok_or_else(|| "--input-json requires a JSON string".to_string())?;
            let _: serde_json::Value =
                serde_json::from_str(json).map_err(|e| format!("invalid --input-json: {e}"))?;
            Ok(json.as_bytes().to_vec())
        }
        other => Err(format!("unknown run option: {other}")),
    }
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
