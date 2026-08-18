//! Live evidence collectors used by promote (build / license / security).
//!
//! Profile + benchmark collection live in command-bus (needs runtime handlers).

use crate::workspace::{adapter_crate_dir, workspace_root_near};
use crate::RegistryError;
use std::path::{Path, PathBuf};
use std::process::Command;
use wvx_ir::{AxisFact, Implementation};

#[derive(Debug, Clone)]
pub struct BuildCollection {
    pub axis: AxisFact,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct LicenseCollection {
    pub axis: AxisFact,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct SecurityCollection {
    pub axis: AxisFact,
    pub detail: String,
}

/// `cargo check -p <adapter crate>` in the discovered workspace.
pub fn collect_build(
    registry_root: &Path,
    imp: &Implementation,
) -> Result<BuildCollection, RegistryError> {
    let Some(crate_name) = imp.adapter.as_ref().map(|a| a.crate_name.clone()) else {
        return Ok(BuildCollection {
            axis: AxisFact::Fail,
            detail: "no adapter crate to check".into(),
        });
    };
    let Some(ws) = workspace_root_near(registry_root) else {
        return Ok(BuildCollection {
            axis: AxisFact::Fail,
            detail: "workspace root not found for cargo check".into(),
        });
    };
    let output = Command::new("cargo")
        .args(["check", "-p", &crate_name, "--offline"])
        .current_dir(&ws)
        .output();
    match output {
        Ok(o) if o.status.success() => Ok(BuildCollection {
            axis: AxisFact::Pass,
            detail: format!("cargo check -p {crate_name} --offline"),
        }),
        Ok(o) => {
            // Offline may fail on a clean cache; retry online once.
            let retry = Command::new("cargo")
                .args(["check", "-p", &crate_name])
                .current_dir(&ws)
                .output();
            match retry {
                Ok(r) if r.status.success() => Ok(BuildCollection {
                    axis: AxisFact::Pass,
                    detail: format!("cargo check -p {crate_name}"),
                }),
                Ok(r) => Ok(BuildCollection {
                    axis: AxisFact::Fail,
                    detail: format!(
                        "cargo check -p {crate_name} failed: {}",
                        tail_utf8(&r.stderr, 400)
                    ),
                }),
                Err(e) => Ok(BuildCollection {
                    axis: AxisFact::Fail,
                    detail: format!(
                        "cargo check retry: {e}; first: {}",
                        tail_utf8(&o.stderr, 200)
                    ),
                }),
            }
        }
        Err(e) => Err(RegistryError::Io(ws.join("Cargo.toml"), e)),
    }
}

/// Read adapter (or workspace) license field.
pub fn collect_license(
    registry_root: &Path,
    imp: &Implementation,
) -> Result<LicenseCollection, RegistryError> {
    let Some(ws) = workspace_root_near(registry_root) else {
        return Ok(LicenseCollection {
            axis: AxisFact::Fail,
            detail: "workspace root not found".into(),
        });
    };
    if let Some(dir) = adapter_crate_dir(&ws, imp) {
        if let Some(lic) = read_toml_license(&dir.join("Cargo.toml")) {
            if license_acceptable(&lic) {
                return Ok(LicenseCollection {
                    axis: AxisFact::Pass,
                    detail: format!("{}: {lic}", dir.display()),
                });
            }
            return Ok(LicenseCollection {
                axis: AxisFact::Fail,
                detail: format!("unacceptable license `{lic}`"),
            });
        }
    }
    if let Some(lic) = read_toml_license(&ws.join("Cargo.toml")) {
        if license_acceptable(&lic) {
            return Ok(LicenseCollection {
                axis: AxisFact::Pass,
                detail: format!("workspace license: {lic}"),
            });
        }
    }
    Ok(LicenseCollection {
        axis: AxisFact::Fail,
        detail: "no license field on adapter crate or workspace".into(),
    })
}

/// Lightweight security collection: `unsafe` scan of adapter crate + optional cargo-deny.
pub fn collect_security(
    registry_root: &Path,
    imp: &Implementation,
) -> Result<SecurityCollection, RegistryError> {
    let Some(ws) = workspace_root_near(registry_root) else {
        return Ok(SecurityCollection {
            axis: AxisFact::Absent,
            detail: "workspace root not found".into(),
        });
    };
    let mut notes = Vec::new();
    let mut failed = false;

    if let Some(dir) = adapter_crate_dir(&ws, imp) {
        let src = dir.join("src");
        if src.is_dir() {
            match scan_unsafe(&src) {
                Ok(0) => notes.push("no `unsafe` in adapter src".into()),
                Ok(n) => {
                    failed = true;
                    notes.push(format!("{n} `unsafe` token(s) in adapter src"));
                }
                Err(e) => notes.push(format!("unsafe scan: {e}")),
            }
        }
    }

    if ws.join("deny.toml").is_file() {
        match Command::new("cargo")
            .args(["deny", "check", "licenses", "--offline"])
            .current_dir(&ws)
            .output()
        {
            Ok(o) if o.status.success() => notes.push("cargo deny check licenses".into()),
            Ok(o)
                if o.status.code() == Some(127)
                    || String::from_utf8_lossy(&o.stderr).contains("no such command") =>
            {
                notes.push("cargo-deny not installed (skipped)".into());
            }
            Ok(o) => {
                // deny may be missing as a subcommand
                let err = tail_utf8(&o.stderr, 200);
                if err.contains("no such command") || err.contains("is not a command") {
                    notes.push("cargo-deny not installed (skipped)".into());
                } else {
                    notes.push(format!("cargo deny: {err}"));
                }
            }
            Err(_) => notes.push("cargo-deny not installed (skipped)".into()),
        }
    }

    Ok(SecurityCollection {
        axis: if failed {
            AxisFact::Fail
        } else if notes.iter().any(|n| n.contains("no `unsafe`")) {
            AxisFact::Pass
        } else {
            AxisFact::Absent
        },
        detail: notes.join("; "),
    })
}

fn license_acceptable(lic: &str) -> bool {
    let l = lic.to_ascii_lowercase();
    l.contains("mit") || l.contains("apache") || l.contains("bsd") || l.contains("isc")
}

fn read_toml_license(path: &Path) -> Option<String> {
    let text = fs_read(path)?;
    // workspace.package.license or package.license
    for key in ["license = ", "license="] {
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with('#') {
                continue;
            }
            if let Some(rest) = t.strip_prefix(key) {
                let v = rest.trim().trim_matches('"').trim_matches('\'');
                if !v.is_empty() && v != "true" {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn fs_read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn scan_unsafe(dir: &Path) -> Result<usize, String> {
    let mut n = 0usize;
    scan_unsafe_walk(dir, &mut n)?;
    Ok(n)
}

fn scan_unsafe_walk(dir: &Path, n: &mut usize) -> Result<(), String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in rd {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            scan_unsafe_walk(&path, n)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            for token in text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
                if token == "unsafe" {
                    *n += 1;
                }
            }
        }
    }
    Ok(())
}

fn tail_utf8(bytes: &[u8], max: usize) -> String {
    let s = String::from_utf8_lossy(bytes);
    let t = s.trim();
    if t.len() <= max {
        t.to_string()
    } else {
        t[t.len() - max..].to_string()
    }
}

/// Resolve workspace path for digest context.
pub fn digest_workspace(registry_root: &Path) -> Option<PathBuf> {
    workspace_root_near(registry_root)
}
