//! Quick capture: append a snippet of text to an inbox note without opening the main window.

use crate::fs_atomic::atomic_write_str;
use crate::vault::{ensure_md_extension, resolve_safe_vault_path};
use std::fs;
use std::path::Path;

/// Appends `text` to the note at `rel_path` and creates the note, with a title, when it is
/// missing. Entries are separated by a blank line. Returns the note's vault-relative path.
pub fn append_to_note(vault_root: &Path, rel_path: &str, text: &str) -> Result<String, String> {
    let entry = text.trim();
    if entry.is_empty() {
        return Err("There is nothing to save.".to_string());
    }
    let md_rel = ensure_md_extension(rel_path);
    let (abs_path, clean_rel) = resolve_safe_vault_path(vault_root, &md_rel)?;

    let mut content = if abs_path.exists() {
        fs::read_to_string(&abs_path)
            .map_err(|e| format!("Cannot read '{clean_rel}': {e}"))?
    } else {
        let title = abs_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Inbox");
        format!("# {title}\n")
    };

    // Entries sit one blank line apart, whatever the note ended with.
    let kept = content.trim_end().len();
    content.truncate(kept);
    if !content.is_empty() {
        content.push_str("\n\n");
    }
    content.push_str(entry);
    content.push('\n');

    atomic_write_str(&abs_path, &content)?;
    Ok(clean_rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_the_note_with_a_title_and_the_first_entry() {
        let dir = tempfile::tempdir().unwrap();
        let rel = append_to_note(dir.path(), "Inbox", "Call the bank").unwrap();
        assert_eq!(rel, "Inbox.md");
        assert_eq!(
            fs::read_to_string(dir.path().join("Inbox.md")).unwrap(),
            "# Inbox\n\nCall the bank\n"
        );
    }

    #[test]
    fn appends_after_existing_text_with_one_blank_line() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Inbox.md"), "# Inbox\n\nfirst   \n\n\n").unwrap();
        append_to_note(dir.path(), "Inbox.md", "  - [ ] second\n  third line \n").unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("Inbox.md")).unwrap(),
            "# Inbox\n\nfirst\n\n- [ ] second\n  third line\n"
        );
    }

    #[test]
    fn writes_into_a_folder_that_does_not_exist_yet() {
        let dir = tempfile::tempdir().unwrap();
        append_to_note(dir.path(), "Capture/Quick.md", "idea").unwrap();
        assert!(dir.path().join("Capture/Quick.md").exists());
    }

    #[test]
    fn rejects_empty_text_and_paths_outside_the_vault() {
        let dir = tempfile::tempdir().unwrap();
        assert!(append_to_note(dir.path(), "Inbox.md", " \n ").is_err());
        assert!(append_to_note(dir.path(), "../Outside.md", "x").is_err());
        assert!(!dir.path().join("Inbox.md").exists());
    }
}
