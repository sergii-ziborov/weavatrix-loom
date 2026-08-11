//! `wvx` — Weavatrix Loom CLI (thin host over the command bus).

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use wvx_command_bus::{
    load_project_path, project_export_rust, project_validate, registry_search, BusError,
};
use wvx_registry_client::LocalRegistry;

fn main() -> ExitCode {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return ExitCode::SUCCESS;
    }

    let cmd = args.remove(0);
    match cmd.as_str() {
        "validate" => cmd_validate(&args),
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
  wvx export-rust <project.wvx.json>
  wvx registry-search <registry-dir> [query]
  wvx version
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
