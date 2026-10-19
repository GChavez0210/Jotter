//! User CSS snippets: `.css` files in Jotter's app data folder, loaded on top of the theme.
//!
//! Snippets live outside the vault so Jotter never writes into it and never picks up
//! Obsidian's `.obsidian/snippets`, which target Obsidian's DOM.

use std::fs;
use std::path::Path;

use crate::types::CssSnippet;

/// Larger files are skipped so one stray file cannot stall the window.
const MAX_SNIPPET_BYTES: u64 = 512 * 1024;

/// Reads every `.css` file directly inside `dir`, sorted by name. Creates `dir` when missing.
pub fn list_snippets(dir: &Path) -> Result<Vec<CssSnippet>, String> {
    fs::create_dir_all(dir)
        .map_err(|e| format!("Cannot create snippets folder {}: {e}", dir.display()))?;
    let entries =
        fs::read_dir(dir).map_err(|e| format!("Cannot read snippets folder {}: {e}", dir.display()))?;

    let mut snippets = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_css = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("css"));
        let small_file = entry
            .metadata()
            .is_ok_and(|m| m.is_file() && m.len() <= MAX_SNIPPET_BYTES);
        if !is_css || !small_file {
            continue;
        }
        let Some(name) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        if let Ok(css) = fs::read_to_string(&path) {
            snippets.push(CssSnippet { name, css });
        }
    }
    snippets.sort_by_key(|s| s.name.to_lowercase());
    Ok(snippets)
}

/// Opens `dir` in the platform file manager.
pub fn reveal_folder(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|e| format!("Cannot create folder {}: {e}", dir.display()))?;
    let program = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Cannot open {}: {e}", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_only_css_files_sorted_by_name() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("snippets");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("wide.css"), ".cm-content { max-width: none; }").unwrap();
        fs::write(dir.join("Accent.CSS"), ":root { --accent: red; }").unwrap();
        fs::write(dir.join("notes.txt"), "not css").unwrap();
        fs::create_dir_all(dir.join("nested.css")).unwrap();

        let names: Vec<String> = list_snippets(&dir).unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["Accent", "wide"]);
    }

    #[test]
    fn creates_missing_folder() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("missing");
        assert!(list_snippets(&dir).unwrap().is_empty());
        assert!(dir.is_dir());
    }
}
