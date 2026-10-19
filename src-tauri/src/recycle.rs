//! Lists and restores vault notes that Jotter moved to the OS trash.
//!
//! The `trash` crate can only list and restore on Windows and Freedesktop systems. On other
//! platforms both functions return an error and the frontend hides the view.

#[cfg(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
mod imp {
    use std::path::{Path, PathBuf};

    use trash::os_limited::{list, metadata, restore_all};
    use trash::{TrashItem, TrashItemSize};

    use crate::types::DeletedEntry;

    /// Returns `path` relative to `root` with `/` separators. Components compare
    /// case-insensitively because the Recycle Bin may report a different case than the vault path.
    fn rel_under_root(root: &Path, path: &Path) -> Option<String> {
        let key = |c: std::path::Component| c.as_os_str().to_string_lossy().to_lowercase();
        let mut path_parts = path.components();
        for root_part in root.components() {
            let path_part = path_parts.next()?;
            if key(root_part) != key(path_part) {
                return None;
            }
        }
        Some(
            path_parts
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/"),
        )
    }

    fn is_listed_entry(rel: &str, is_folder: bool) -> bool {
        if rel.is_empty() || rel.split('/').any(|part| part.starts_with('.')) {
            return false;
        }
        is_folder || rel.to_lowercase().ends_with(".md")
    }

    fn to_entry(root: &Path, item: &TrashItem) -> Option<DeletedEntry> {
        let original = item.original_path();
        let rel = rel_under_root(root, &original)?;
        let is_folder = matches!(
            metadata(item).map(|m| m.size),
            Ok(TrashItemSize::Entries(_))
        );
        if !is_listed_entry(&rel, is_folder) {
            return None;
        }
        Some(DeletedEntry {
            id: item.id.to_string_lossy().into_owned(),
            name: item.name.to_string_lossy().into_owned(),
            original_rel_path: rel,
            deleted_ms: item.time_deleted.saturating_mul(1000),
            is_folder,
        })
    }

    pub fn list_deleted(vault_root: &Path) -> Result<Vec<DeletedEntry>, String> {
        let items = list().map_err(|e| format!("Could not read the Recycle Bin: {e}"))?;
        let mut entries: Vec<DeletedEntry> = items
            .iter()
            .filter_map(|item| to_entry(vault_root, item))
            .collect();
        entries.sort_by(|a, b| b.deleted_ms.cmp(&a.deleted_ms));
        Ok(entries)
    }

    pub fn restore(vault_root: &Path, id: &str) -> Result<String, String> {
        let items = list().map_err(|e| format!("Could not read the Recycle Bin: {e}"))?;
        let item = items
            .into_iter()
            .find(|item| item.id.to_string_lossy() == id)
            .ok_or_else(|| "That item is no longer in the Recycle Bin.".to_string())?;
        let entry = to_entry(vault_root, &item)
            .ok_or_else(|| "That item does not belong to this vault.".to_string())?;

        let target: PathBuf = item.original_path();
        crate::vault::ensure_vault_containment(
            vault_root,
            &vault_root.join(&entry.original_rel_path),
        )?;
        if target.exists() {
            return Err(format!(
                "Cannot restore '{}': a file or folder with that name already exists. Rename it, then try again.",
                entry.original_rel_path
            ));
        }
        restore_all([item]).map_err(|e| format!("Restore failed: {e}"))?;
        Ok(entry.original_rel_path)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn rel_paths_match_vault_root_case_insensitively() {
            let root = Path::new("C:/Notes/Work");
            assert_eq!(
                rel_under_root(root, Path::new("c:/notes/work/Daily/Today.md")).as_deref(),
                Some("Daily/Today.md")
            );
            assert_eq!(
                rel_under_root(root, Path::new("C:/Notes/Workshop/a.md")),
                None
            );
        }

        #[test]
        fn lists_notes_and_folders_but_not_hidden_or_other_files() {
            assert!(is_listed_entry("Daily/Today.md", false));
            assert!(is_listed_entry("Projects", true));
            assert!(!is_listed_entry("attachments/a.png", false));
            assert!(!is_listed_entry(".obsidian/app.json", false));
            assert!(!is_listed_entry("", true));
        }
    }
}

#[cfg(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
pub use imp::{list_deleted, restore};

#[cfg(not(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
)))]
pub fn list_deleted(
    _vault_root: &std::path::Path,
) -> Result<Vec<crate::types::DeletedEntry>, String> {
    Err("Listing the trash is not supported on this platform.".to_string())
}

#[cfg(not(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
)))]
pub fn restore(_vault_root: &std::path::Path, _id: &str) -> Result<String, String> {
    Err("Restoring from the trash is not supported on this platform.".to_string())
}
