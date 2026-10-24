//! Pinned notes, kept per vault in the app data folder.
//!
//! Pins are the user's own choice, not derivable from the notes, so they never live in the
//! rebuildable index cache and never inside the vault, where a sync client would carry one
//! computer's pins to another (decision 3).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fs_atomic::atomic_write_str;

/// On-disk shape. The vault path is recorded for a human reading the file, never read back.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PinnedFile {
    vault: String,
    pinned: Vec<String>,
}

/// Per-vault pins file, named by the same key as the vault's index database.
pub fn pins_path(app_data_dir: &Path, vault_root: &Path) -> PathBuf {
    app_data_dir
        .join("vaults")
        .join(format!("{}.pins.json", crate::platform::vault_key(vault_root)))
}

/// Reads the stored pins as written. Callers that display them filter with [`existing_pins`];
/// callers that rewrite paths need the raw list, since a renamed note's old path is already gone.
pub fn load_pins(pins_file: &Path) -> Vec<String> {
    let Ok(raw) = std::fs::read_to_string(pins_file) else {
        return Vec::new();
    };
    serde_json::from_str::<PinnedFile>(&raw)
        .map(|parsed| parsed.pinned)
        .unwrap_or_default()
}

/// Keeps only pins whose note is still on disk. The file keeps the rest, so a note restored
/// from the trash comes back pinned.
pub fn existing_pins(pinned: &[String], vault_root: &Path) -> Vec<String> {
    pinned
        .iter()
        .filter(|rel| vault_root.join(rel).is_file())
        .cloned()
        .collect()
}

pub fn save_pins(pins_file: &Path, vault_root: &Path, pinned: &[String]) -> Result<(), String> {
    if let Some(parent) = pins_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create the Jotter vault settings folder: {e}"))?;
    }
    let payload = PinnedFile {
        vault: vault_root.to_string_lossy().into_owned(),
        pinned: pinned.to_vec(),
    };
    let json = serde_json::to_string_pretty(&payload)
        .map_err(|e| format!("Cannot serialize pinned notes: {e}"))?;
    atomic_write_str(pins_file, &json)
}

/// Adds or removes one pin. New pins go to the end, so the list never reshuffles itself.
pub fn set_pinned(pinned: &[String], rel_path: &str, pin: bool) -> Vec<String> {
    let mut next: Vec<String> = pinned
        .iter()
        .filter(|existing| !paths_equal(existing, rel_path))
        .cloned()
        .collect();
    if pin {
        next.push(rel_path.to_string());
    }
    next
}

/// Follows a renamed or moved note, and every pin inside a renamed folder.
pub fn rename_pins(pinned: &[String], old_rel: &str, new_rel: &str) -> Vec<String> {
    pinned
        .iter()
        .map(|pin| {
            if paths_equal(pin, old_rel) {
                new_rel.to_string()
            } else if let Some(rest) = strip_folder_prefix(pin, old_rel) {
                format!("{new_rel}/{rest}")
            } else {
                pin.clone()
            }
        })
        .collect()
}

/// Windows and macOS open `Note.md` and `note.md` as one file, so pins compare the same way
/// the vault does. Linux keeps them distinct.
fn paths_equal(a: &str, b: &str) -> bool {
    if cfg!(target_os = "linux") {
        a == b
    } else {
        a.eq_ignore_ascii_case(b)
    }
}

/// `Some(rest)` when `pin` sits inside the `folder` directory.
fn strip_folder_prefix<'a>(pin: &'a str, folder: &str) -> Option<&'a str> {
    let prefix_len = folder.len() + 1;
    if pin.len() <= prefix_len {
        return None;
    }
    let (head, rest) = pin.split_at(prefix_len);
    if paths_equal(head, &format!("{folder}/")) {
        Some(rest)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinning_appends_and_unpinning_removes_without_reordering() {
        let pins = set_pinned(&[], "a.md", true);
        let pins = set_pinned(&pins, "b.md", true);
        let pins = set_pinned(&pins, "c.md", true);
        assert_eq!(pins, vec!["a.md", "b.md", "c.md"]);
        let pins = set_pinned(&pins, "b.md", false);
        assert_eq!(pins, vec!["a.md", "c.md"]);
        // Pinning twice keeps one entry.
        let pins = set_pinned(&pins, "a.md", true);
        assert_eq!(pins, vec!["c.md", "a.md"]);
    }

    #[test]
    fn renaming_a_note_or_its_folder_carries_the_pin() {
        let pins = vec![
            "Work/Report.md".to_string(),
            "Work/Deep/Notes.md".to_string(),
            "Personal/Diary.md".to_string(),
        ];
        let renamed_note = rename_pins(&pins, "Work/Report.md", "Work/Summary.md");
        assert_eq!(renamed_note[0], "Work/Summary.md");
        assert_eq!(renamed_note[2], "Personal/Diary.md");

        let renamed_folder = rename_pins(&pins, "Work", "Archive");
        assert_eq!(
            renamed_folder,
            vec!["Archive/Report.md", "Archive/Deep/Notes.md", "Personal/Diary.md"]
        );
    }

    #[test]
    fn a_folder_rename_leaves_a_similarly_named_sibling_alone() {
        let pins = vec!["Work/a.md".to_string(), "Workshop/b.md".to_string()];
        assert_eq!(
            rename_pins(&pins, "Work", "Archive"),
            vec!["Archive/a.md", "Workshop/b.md"]
        );
    }

    #[test]
    fn pins_round_trip_through_the_file_and_skip_missing_notes() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(vault.join("Work")).unwrap();
        std::fs::write(vault.join("Work/a.md"), "a").unwrap();
        let pins_file = pins_path(dir.path(), &vault);

        let stored = vec!["Work/a.md".to_string(), "Gone.md".to_string()];
        save_pins(&pins_file, &vault, &stored).unwrap();
        // The file keeps every pin, so restoring Gone.md from the trash brings its pin back.
        assert_eq!(load_pins(&pins_file), stored);
        assert_eq!(existing_pins(&load_pins(&pins_file), &vault), vec!["Work/a.md"]);
    }

    #[test]
    fn each_vault_keeps_its_own_pins_file() {
        let data = Path::new("C:/data");
        assert_ne!(
            pins_path(data, Path::new("C:/Notes/Work")),
            pins_path(data, Path::new("C:/Notes/Personal"))
        );
    }
}
