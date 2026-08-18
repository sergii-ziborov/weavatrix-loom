//! Materialize a temporary Registry subset (tests + e2e). Never mutates `registry-dev`.

use crate::RegistryError;
use std::fs;
use std::path::{Path, PathBuf};

/// Copy selected capabilities, profiles, and implementations into `dest`.
///
/// `dest` is created (or reused). Existing files in `dest` are left as-is
/// unless overwritten by a copied name.
pub fn materialize_temp_registry(
    src: &Path,
    dest: &Path,
    capabilities: &[&str],
    profiles: &[&str],
    implementations: &[&str],
) -> Result<PathBuf, RegistryError> {
    fs::create_dir_all(dest).map_err(|e| RegistryError::Io(dest.to_path_buf(), e))?;
    for (subdir, names, ext) in [
        ("capabilities", capabilities, ".json"),
        ("profiles", profiles, ".json"),
        ("implementations", implementations, ".json"),
    ] {
        let out_dir = dest.join(subdir);
        fs::create_dir_all(&out_dir).map_err(|e| RegistryError::Io(out_dir.clone(), e))?;
        for name in names.iter() {
            let file = if name.ends_with(".json") {
                name.to_string()
            } else {
                format!("{name}{ext}")
            };
            let from = src.join(subdir).join(&file);
            let to = out_dir.join(&file);
            if !from.is_file() {
                return Err(RegistryError::Parse(
                    from,
                    format!("temp registry source missing {subdir}/{file}"),
                ));
            }
            fs::copy(&from, &to).map_err(|e| RegistryError::Io(from, e))?;
        }
    }
    let ev = dest.join("evidence").join("artifacts");
    fs::create_dir_all(&ev).map_err(|e| RegistryError::Io(ev, e))?;
    Ok(dest.to_path_buf())
}

/// Snapshot filesystem entries under `root` (relative paths, files only).
pub fn snapshot_files(root: &Path) -> Result<Vec<(String, u64)>, RegistryError> {
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, u64)>) -> Result<(), RegistryError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|e| RegistryError::Io(dir.to_path_buf(), e))? {
        let entry = entry.map_err(|e| RegistryError::Io(dir.to_path_buf(), e))?;
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name == "target" || name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let meta = fs::metadata(&path).map_err(|e| RegistryError::Io(path.clone(), e))?;
            out.push((rel, meta.len()));
        }
    }
    Ok(())
}
