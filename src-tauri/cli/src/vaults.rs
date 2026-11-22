//! The vaults the Jotter app has opened, read from its recent list, so an assistant can list
//! them and switch between them. Only those vaults, plus the one the CLI started on, can be
//! opened this way: an assistant cannot point the CLI at an arbitrary folder.

use std::path::{Path, PathBuf};

use jotter_core::types::RecentVault;
use serde_json::{json, Value};

fn recent(data_dir: &Path) -> Vec<RecentVault> {
    jotter_core::vault::load_recent_vaults(&data_dir.join("recent_vaults.json"))
}

/// The vault at the top of the app's recent list. The list skips folders that no longer exist.
pub fn last_opened(data_dir: &Path) -> Option<PathBuf> {
    recent(data_dir).into_iter().next().map(|vault| PathBuf::from(vault.path))
}

/// Whether two paths name the same folder. Falls back to a case-insensitive comparison when a
/// path cannot be resolved, as the app's recent list does.
fn same_folder(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()),
    }
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// The app's recent vaults, newest first, plus `start` when the list lacks it.
fn known(data_dir: &Path, start: &Path) -> Vec<(PathBuf, String, Option<i64>)> {
    let mut vaults: Vec<(PathBuf, String, Option<i64>)> = recent(data_dir)
        .into_iter()
        .map(|v| (PathBuf::from(v.path), v.name, Some(v.last_opened_ms)))
        .collect();
    if !vaults.iter().any(|(path, _, _)| same_folder(path, start)) {
        vaults.push((start.to_path_buf(), name_of(start), None));
    }
    vaults
}

/// Lists the vaults that `open` accepts, marking the one in use.
pub fn list(data_dir: &Path, start: &Path, current: &Path) -> Value {
    let vaults: Vec<Value> = known(data_dir, start)
        .into_iter()
        .map(|(path, name, last_opened_ms)| {
            json!({
                "name": name,
                "path": path.to_string_lossy(),
                "lastOpenedMs": last_opened_ms,
                "current": same_folder(&path, current),
            })
        })
        .collect();
    Value::Array(vaults)
}

/// Finds a known vault by path or by name. A name shared by two vaults needs the path.
pub fn resolve(data_dir: &Path, start: &Path, wanted: &str) -> Result<PathBuf, String> {
    let wanted = wanted.trim();
    let vaults = known(data_dir, start);
    if let Some((path, _, _)) = vaults.iter().find(|(path, _, _)| same_folder(path, Path::new(wanted))) {
        return Ok(path.clone());
    }
    let named: Vec<&PathBuf> =
        vaults.iter().filter(|(_, name, _)| name.eq_ignore_ascii_case(wanted)).map(|(path, _, _)| path).collect();
    match named.as_slice() {
        [path] => Ok((*path).clone()),
        [] => Err(format!(
            "'{wanted}' is not a vault Jotter has opened. Call list_vaults for the vaults you can open, or open the folder in the Jotter app first"
        )),
        _ => Err(format!("More than one vault is named '{wanted}'. Give its path from list_vaults")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let work = temp.path().join("Work");
        let home = temp.path().join("Home");
        for dir in [&data, &work, &home] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let recent = data.join("recent_vaults.json");
        jotter_core::vault::record_recent_vault(&recent, &home);
        jotter_core::vault::record_recent_vault(&recent, &work);
        (temp, data, work, home)
    }

    #[test]
    fn last_opened_is_the_newest_existing_vault() {
        let (_temp, data, work, home) = setup();
        assert_eq!(last_opened(&data), Some(work.clone()));
        std::fs::remove_dir(&work).unwrap();
        assert_eq!(last_opened(&data), Some(home));
    }

    #[test]
    fn list_marks_the_current_vault_and_includes_the_start_vault() {
        let (temp, data, work, home) = setup();
        let other = temp.path().join("Other");
        std::fs::create_dir_all(&other).unwrap();
        let listed = list(&data, &other, &home);
        let names: Vec<&str> = listed.as_array().unwrap().iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["Work", "Home", "Other"]);
        let current: Vec<bool> = listed.as_array().unwrap().iter().map(|v| v["current"].as_bool().unwrap()).collect();
        assert_eq!(current, [false, true, false]);
        assert!(list(&data, &work, &work).as_array().unwrap().len() == 2);
    }

    #[test]
    fn resolve_accepts_known_names_and_paths_only() {
        let (temp, data, work, home) = setup();
        assert_eq!(resolve(&data, &work, "home").unwrap(), home);
        assert_eq!(resolve(&data, &work, &work.to_string_lossy()).unwrap(), work);
        let stranger = temp.path().join("Stranger");
        std::fs::create_dir_all(&stranger).unwrap();
        assert!(resolve(&data, &work, &stranger.to_string_lossy()).is_err());
        assert!(resolve(&data, &work, "Nope").is_err());
    }
}
