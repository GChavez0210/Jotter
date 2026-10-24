//! App settings, kept in one JSON file in the app data folder.
//!
//! The WebView's local storage dies with a reinstall or a profile reset, and it cannot move to
//! another computer. The frontend keeps a string-to-string map and this module stores it.

use std::path::{Path, PathBuf};

use crate::fs_atomic::atomic_write_str;

/// An export or import file larger than this is not a Jotter settings file.
pub const MAX_SETTINGS_BYTES: u64 = 1024 * 1024;

#[derive(Debug, serde::Serialize)]
pub struct SettingsFileError {
    pub code: &'static str,
    pub path: String,
    pub detail: String,
}

impl SettingsFileError {
    pub fn new(code: &'static str, path: &Path, detail: impl ToString) -> Self {
        Self { code, path: path.to_string_lossy().into_owned(), detail: detail.to_string() }
    }
}

pub fn settings_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("settings.json")
}

/// Returns the stored settings, or `None` before the first save.
pub fn load(settings_file: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(settings_file) {
        Ok(raw) => Ok(Some(raw)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("Cannot read {}: {err}", settings_file.display())),
    }
}

/// Writes `json` after checking that it is a JSON object, so a bad call cannot wipe the file.
pub fn save(settings_file: &Path, json: &str) -> Result<(), String> {
    if json.len() as u64 > MAX_SETTINGS_BYTES {
        return Err("Settings are too large to save".to_string());
    }
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(serde_json::Value::Object(_)) => {}
        _ => return Err("Settings must be a JSON object".to_string()),
    }
    atomic_write_str(settings_file, json)
}

/// Reads a settings file the user picked.
pub fn read_import(path: &Path) -> Result<String, SettingsFileError> {
    let size = std::fs::metadata(path)
        .map_err(|e| SettingsFileError::new("read", path, e))?
        .len();
    if size > MAX_SETTINGS_BYTES {
        return Err(SettingsFileError::new("too_large", path, "Settings files must be at most 1 MiB"));
    }
    std::fs::read_to_string(path).map_err(|e| SettingsFileError::new("read", path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jotter-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_returns_none_before_the_first_save() {
        let dir = temp_dir("none");
        assert_eq!(load(&settings_path(&dir)).unwrap(), None);
    }

    #[test]
    fn saved_settings_load_back() {
        let dir = temp_dir("roundtrip");
        let file = settings_path(&dir);
        save(&file, r#"{"jotter.themePref":"dark"}"#).unwrap();
        assert_eq!(load(&file).unwrap().as_deref(), Some(r#"{"jotter.themePref":"dark"}"#));
    }

    #[test]
    fn save_rejects_anything_but_an_object_and_keeps_the_old_file() {
        let dir = temp_dir("reject");
        let file = settings_path(&dir);
        save(&file, r#"{"a":"1"}"#).unwrap();
        assert!(save(&file, "[1,2]").is_err());
        assert!(save(&file, "not json").is_err());
        assert_eq!(load(&file).unwrap().as_deref(), Some(r#"{"a":"1"}"#));
    }

    #[test]
    fn import_refuses_a_file_over_the_size_limit() {
        let dir = temp_dir("big");
        let file = dir.join("big.json");
        std::fs::write(&file, vec![b' '; (MAX_SETTINGS_BYTES + 1) as usize]).unwrap();
        assert_eq!(read_import(&file).unwrap_err().code, "too_large");
    }
}
