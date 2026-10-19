use std::cmp::Ordering;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use crate::fs_atomic::atomic_write_str;
use crate::index::system_time_to_ms;
use crate::types::{
    NoteDocument, RecentVault, RenameEntryResult, VaultNode, VaultNodeKind, VaultSnapshot,
};

const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
    "LPT9",
];

const WINDOWS_INVALID_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Validates a single path segment (file or folder name) against Windows naming rules:
/// - Non-empty
/// - No leading dot (`.obsidian`, `.git`, `.hidden`)
/// - No trailing space or trailing period (checked both on the full segment and on the stem before `.md`)
/// - No invalid Windows characters (`< > : " / \ | ? *` or control characters)
/// - Not a Windows reserved device name (`CON`, `PRN`, `AUX`, `NUL`, `COM0-9`, `LPT0-9`, with or without extension)
pub fn validate_windows_entry_name(raw_segment: &str) -> Result<(), String> {
    if raw_segment.is_empty() || raw_segment.trim().is_empty() {
        return Err("Name cannot be empty.".to_string());
    }

    if raw_segment.ends_with(' ') || raw_segment.ends_with('.') {
        return Err("Windows names cannot end with a space or a period.".to_string());
    }

    // If the segment has an extension (e.g. `foo .md` or `foo..md`), also check the stem before `.md`
    if let Some(stem) = raw_segment
        .strip_suffix(".md")
        .or_else(|| raw_segment.strip_suffix(".MD"))
    {
        if stem.is_empty() || stem.trim().is_empty() {
            return Err("Note name cannot be empty.".to_string());
        }
        if stem.ends_with(' ') || stem.ends_with('.') {
            return Err("Windows names cannot end with a space or a period.".to_string());
        }
    }

    let trimmed = raw_segment.trim();
    if trimmed.starts_with('.') {
        return Err(format!(
            "Names starting with a dot (such as '{trimmed}') are reserved."
        ));
    }

    for ch in raw_segment.chars() {
        if WINDOWS_INVALID_CHARS.contains(&ch) || ch.is_control() {
            return Err(format!(
                "Name contains invalid Windows character '{ch}'. Avoid < > : \" / \\ | ? *"
            ));
        }
    }

    // Check reserved device names (both full segment and stem before first '.')
    let stem_before_dot = trimmed.split('.').next().unwrap_or(trimmed).trim();
    let upper = stem_before_dot.to_ascii_uppercase();
    if WINDOWS_RESERVED_NAMES.contains(&upper.as_str()) {
        return Err(format!(
            "'{stem_before_dot}' is a reserved Windows device name."
        ));
    }

    Ok(())
}

/// Normalizes a relative vault path to forward slashes (`/`) without leading or trailing slashes.
/// Preserves any trailing spaces/dots on individual path segments so `validate_windows_entry_name` can catch them.
pub fn normalize_rel_path(rel_path: &str) -> String {
    rel_path
        .replace('\\', "/")
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string()
}

/// Validates and resolves a relative path inside `vault_root`.
/// Rejects path traversal (`..`), absolute paths, `.obsidian` or dot-prefixed segments,
/// and invalid Windows entry names.
pub fn resolve_safe_vault_path(
    vault_root: &Path,
    rel_path: &str,
) -> Result<(PathBuf, String), String> {
    let normalized = normalize_rel_path(rel_path);
    if normalized.trim().is_empty() {
        return Err("Relative path cannot be empty.".to_string());
    }

    // Validate each slash-separated segment before Path::components (since Path on Windows can strip trailing dots)
    let raw_parts: Vec<&str> = normalized.split('/').collect();
    let mut clean_parts = Vec::with_capacity(raw_parts.len());

    for raw_part in raw_parts {
        if raw_part == "." || raw_part == ".." {
            return Err(format!(
                "Path traversal or non-relative path segment is not allowed: {rel_path}"
            ));
        }
        validate_windows_entry_name(raw_part)?;
        let trimmed_start = raw_part.trim_start();
        clean_parts.push(trimmed_start.to_string());
    }

    let rel = Path::new(&normalized);
    for comp in rel.components() {
        if !matches!(comp, Component::Normal(_)) {
            return Err(format!(
                "Path traversal or non-relative path segment is not allowed: {rel_path}"
            ));
        }
    }

    let mut full_path = vault_root.to_path_buf();
    for part in &clean_parts {
        full_path.push(part);
    }

    Ok((full_path, clean_parts.join("/")))
}

/// Ensures a relative note path ends with `.md` (case-insensitive), validating raw segments first
/// so trailing dots/spaces before `.md` are caught.
pub fn ensure_md_extension(rel_path: &str) -> String {
    let norm = normalize_rel_path(rel_path);
    if norm.len() >= 3 && norm[norm.len() - 3..].eq_ignore_ascii_case(".md") {
        norm
    } else {
        format!("{norm}.md")
    }
}

/// Scans `vault_root` and builds a `VaultSnapshot` containing the recursive folder/note tree.
/// Strictly ignores `.obsidian` and any dot-prefixed files or folders.
pub fn scan_vault_snapshot(vault_root: &Path) -> Result<VaultSnapshot, String> {
    if !vault_root.exists() || !vault_root.is_dir() {
        return Err(format!(
            "Vault directory does not exist: {}",
            vault_root.display()
        ));
    }

    let mut note_count = 0;
    let tree = scan_dir_nodes(vault_root, vault_root, &mut note_count)?;
    let name = vault_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Vault")
        .to_string();

    Ok(VaultSnapshot {
        root_path: vault_root.to_string_lossy().into_owned(),
        name,
        tree,
        note_count,
    })
}

fn scan_dir_nodes(
    vault_root: &Path,
    dir: &Path,
    note_count: &mut usize,
) -> Result<Vec<VaultNode>, String> {
    let entries =
        fs::read_dir(dir).map_err(|e| format!("Failed to read directory {}: {e}", dir.display()))?;

    let mut nodes = Vec::new();

    for entry_res in entries {
        let entry = match entry_res {
            Ok(e) => e,
            Err(_) => continue,
        };
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy().into_owned();

        // Ignore .obsidian, .git, and atomic write temporary files (.note.md.*.tmp)
        if name_str.starts_with('.') {
            continue;
        }

        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let modified_ms = metadata
            .modified()
            .map(system_time_to_ms)
            .unwrap_or(0);
        let created_ms = metadata
            .created()
            .map(system_time_to_ms)
            .unwrap_or(modified_ms);

        let rel = path
            .strip_prefix(vault_root)
            .map_err(|e| format!("Failed to compute relative path: {e}"))?;
        let rel_path = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");

        if metadata.is_dir() {
            let children = scan_dir_nodes(vault_root, &path, note_count)?;
            nodes.push(VaultNode {
                name: name_str,
                rel_path,
                kind: VaultNodeKind::Folder,
                modified_ms,
                created_ms,
                children,
            });
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            *note_count += 1;
            nodes.push(VaultNode {
                name: name_str,
                rel_path,
                kind: VaultNodeKind::File,
                modified_ms,
                created_ms,
                children: Vec::new(),
            });
        }
    }

    // Sort folders first, then case-insensitive alphabetical by name
    nodes.sort_by(|a, b| match (a.kind, b.kind) {
        (VaultNodeKind::Folder, VaultNodeKind::File) => Ordering::Less,
        (VaultNodeKind::File, VaultNodeKind::Folder) => Ordering::Greater,
        _ => a
            .name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name)),
    });

    Ok(nodes)
}

/// Reads a `.md` note from `vault_root` without modifying it on disk.
pub fn read_vault_note(vault_root: &Path, rel_path: &str) -> Result<NoteDocument, String> {
    let (abs_path, clean_rel) = resolve_safe_vault_path(vault_root, rel_path)?;
    if !abs_path.exists() || !abs_path.is_file() {
        return Err(format!("Note does not exist: {clean_rel}"));
    }

    let content = fs::read_to_string(&abs_path)
        .map_err(|e| format!("Failed to read note {clean_rel}: {e}"))?;
    let metadata = fs::metadata(&abs_path)
        .map_err(|e| format!("Failed to read metadata for {clean_rel}: {e}"))?;

    let modified_ms = metadata
        .modified()
        .map(system_time_to_ms)
        .unwrap_or(0);
    let created_ms = metadata
        .created()
        .map(system_time_to_ms)
        .unwrap_or(modified_ms);
    let title = abs_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();

    Ok(NoteDocument {
        rel_path: clean_rel,
        title,
        content,
        modified_ms,
        created_ms,
    })
}

/// Creates a new `.md` note atomically inside `vault_root`. Fails if the note already exists.
pub fn create_vault_note(
    vault_root: &Path,
    rel_path: &str,
    initial_content: &str,
) -> Result<(VaultSnapshot, NoteDocument), String> {
    let md_rel = ensure_md_extension(rel_path);
    let (abs_path, clean_rel) = resolve_safe_vault_path(vault_root, &md_rel)?;

    if abs_path.exists() {
        return Err(format!("A note or folder already exists at '{clean_rel}'"));
    }

    atomic_write_str(&abs_path, initial_content)?;
    let doc = read_vault_note(vault_root, &clean_rel)?;
    let snapshot = scan_vault_snapshot(vault_root)?;
    Ok((snapshot, doc))
}

/// Saves user edits to an existing or new `.md` note atomically.
/// Returns `(NoteDocument, bool)` where the boolean indicates whether disk bytes actually changed.
/// If the file on disk already has identical bytes, the file is NOT touched.
pub fn save_vault_note(
    vault_root: &Path,
    rel_path: &str,
    content: &str,
) -> Result<(NoteDocument, bool), String> {
    let md_rel = ensure_md_extension(rel_path);
    let (abs_path, clean_rel) = resolve_safe_vault_path(vault_root, &md_rel)?;

    if abs_path.exists() {
        if let Ok(existing) = fs::read_to_string(&abs_path) {
            if existing == content {
                let doc = read_vault_note(vault_root, &clean_rel)?;
                return Ok((doc, false));
            }
        }
    }

    atomic_write_str(&abs_path, content)?;
    let doc = read_vault_note(vault_root, &clean_rel)?;
    Ok((doc, true))
}

/// Creates a new folder inside `vault_root`.
pub fn create_vault_folder(vault_root: &Path, rel_path: &str) -> Result<VaultSnapshot, String> {
    let (abs_path, clean_rel) = resolve_safe_vault_path(vault_root, rel_path)?;
    if abs_path.exists() {
        return Err(format!("'{clean_rel}' already exists"));
    }
    fs::create_dir_all(&abs_path)
        .map_err(|e| format!("Failed to create folder '{clean_rel}': {e}"))?;
    scan_vault_snapshot(vault_root)
}

/// Renames a note or folder inside `vault_root`.
pub fn rename_vault_entry(
    vault_root: &Path,
    old_rel_path: &str,
    new_rel_path: &str,
) -> Result<RenameEntryResult, String> {
    let (old_abs, clean_old) = resolve_safe_vault_path(vault_root, old_rel_path)?;
    if !old_abs.exists() {
        return Err(format!("Entry to rename does not exist: '{clean_old}'"));
    }

    let adjusted_new = if old_abs.is_file() {
        ensure_md_extension(new_rel_path)
    } else {
        normalize_rel_path(new_rel_path)
    };
    let (new_abs, clean_new) = resolve_safe_vault_path(vault_root, &adjusted_new)?;

    if clean_old == clean_new {
        let snapshot = scan_vault_snapshot(vault_root)?;
        return Ok(RenameEntryResult {
            snapshot,
            old_rel_path: clean_old,
            new_rel_path: clean_new,
        });
    }

    // Prevent overwriting a different file (allow case-only renames of the same file on Windows)
    if new_abs.exists() && clean_old.to_lowercase() != clean_new.to_lowercase() {
        return Err(format!("Target path '{clean_new}' already exists"));
    }

    if let Some(parent) = new_abs.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent directory: {e}"))?;
        }
    }

    fs::rename(&old_abs, &new_abs)
        .map_err(|e| format!("Failed to rename '{clean_old}' to '{clean_new}': {e}"))?;

    let snapshot = scan_vault_snapshot(vault_root)?;
    Ok(RenameEntryResult {
        snapshot,
        old_rel_path: clean_old,
        new_rel_path: clean_new,
    })
}

/// Sends a note or folder inside `vault_root` to the Windows Recycle Bin via the `trash` crate.
pub fn delete_vault_entry(vault_root: &Path, rel_path: &str) -> Result<VaultSnapshot, String> {
    let (abs_path, clean_rel) = resolve_safe_vault_path(vault_root, rel_path)?;
    if !abs_path.exists() {
        return Err(format!("Entry to delete does not exist: '{clean_rel}'"));
    }

    trash::delete(&abs_path)
        .map_err(|e| format!("Failed to move '{clean_rel}' to Recycle Bin: {e}"))?;

    scan_vault_snapshot(vault_root)
}

/// Loads the persisted list of recent vaults from `storage_file`, pruning non-existent directories.
pub fn load_recent_vaults(storage_file: &Path) -> Vec<RecentVault> {
    let Ok(raw) = fs::read_to_string(storage_file) else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_str::<Vec<RecentVault>>(&raw) else {
        return Vec::new();
    };
    parsed
        .into_iter()
        .filter(|v| Path::new(&v.path).is_dir())
        .collect()
}

/// Records `vault_root` at the top of the recent vaults list (up to 10 entries) and persists atomically.
pub fn record_recent_vault(storage_file: &Path, vault_root: &Path) -> Vec<RecentVault> {
    let canonical_str = vault_root.to_string_lossy().into_owned();
    let name = vault_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Vault")
        .to_string();
    let now_ms = system_time_to_ms(SystemTime::now());

    let mut list = load_recent_vaults(storage_file);
    list.retain(|v| !v.path.eq_ignore_ascii_case(&canonical_str));
    list.insert(
        0,
        RecentVault {
            path: canonical_str,
            name,
            last_opened_ms: now_ms,
        },
    );
    list.truncate(10);

    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = atomic_write_str(storage_file, &json);
    }

    list
}

/// Removes a vault path from the recent vaults list and persists atomically.
pub fn remove_recent_vault(storage_file: &Path, path_to_remove: &str) -> Vec<RecentVault> {
    let mut list = load_recent_vaults(storage_file);
    list.retain(|v| !v.path.eq_ignore_ascii_case(path_to_remove));
    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = atomic_write_str(storage_file, &json);
    }
    list
}

/// Opens the native Windows folder picker dialog (`SHBrowseForFolderW`) and returns the chosen path,
/// or `None` if the user cancelled.
#[cfg(target_os = "windows")]
pub fn pick_folder_dialog_win32(title: &str) -> Option<PathBuf> {
    use std::ffi::{c_void, OsString};
    use std::os::windows::ffi::OsStringExt;
    use std::ptr;

    #[repr(C)]
    struct BrowseInfoW {
        hwnd_owner: *mut c_void,
        pidl_root: *const c_void,
        psz_display_name: *mut u16,
        lpsz_title: *const u16,
        ul_flags: u32,
        lpfn: *const c_void,
        l_param: isize,
        i_image: i32,
    }

    const BIF_RETURNONLYFSDIRS: u32 = 0x0000_0001;
    const BIF_NEWDIALOGSTYLE: u32 = 0x0000_0040;
    const BIF_USENEWUI: u32 = BIF_NEWDIALOGSTYLE | 0x0000_0010;
    const COINIT_APARTMENTTHREADED: u32 = 0x2;

    #[link(name = "shell32")]
    extern "system" {
        fn SHBrowseForFolderW(lpbi: *const BrowseInfoW) -> *mut c_void;
        fn SHGetPathFromIDListW(pidl: *const c_void, psz_path: *mut u16) -> i32;
    }

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(pv_reserved: *mut c_void, dw_co_init: u32) -> i32;
        fn CoTaskMemFree(pv: *mut c_void);
    }

    let mut display_buf = [0u16; 260];
    let mut path_buf = [0u16; 260];
    let title_wide: Vec<u16> = format!("{title}\0").encode_utf16().collect();

    // SAFETY: Standard Win32 shell folder picker invocation with COM apartment init and PIDL cleanup.
    unsafe {
        let _ = CoInitializeEx(ptr::null_mut(), COINIT_APARTMENTTHREADED);
        let bi = BrowseInfoW {
            hwnd_owner: ptr::null_mut(),
            pidl_root: ptr::null(),
            psz_display_name: display_buf.as_mut_ptr(),
            lpsz_title: title_wide.as_ptr(),
            ul_flags: BIF_RETURNONLYFSDIRS | BIF_USENEWUI,
            lpfn: ptr::null(),
            l_param: 0,
            i_image: 0,
        };

        let pidl = SHBrowseForFolderW(&bi);
        if pidl.is_null() {
            return None;
        }

        let ok = SHGetPathFromIDListW(pidl, path_buf.as_mut_ptr());
        CoTaskMemFree(pidl);

        if ok == 0 {
            return None;
        }

        let len = path_buf
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(path_buf.len());
        let os_str = OsString::from_wide(&path_buf[..len]);
        Some(PathBuf::from(os_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_and_dot_obsidian_paths() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        assert!(resolve_safe_vault_path(root, "../outside.md").is_err());
        assert!(resolve_safe_vault_path(root, ".obsidian/app.json").is_err());
        assert!(resolve_safe_vault_path(root, "folder/.hidden.md").is_err());

        let (_, clean) = resolve_safe_vault_path(root, "Projects\\Jotter\\Roadmap.md").unwrap();
        assert_eq!(clean, "Projects/Jotter/Roadmap.md");
    }

    #[test]
    fn rejects_windows_reserved_names_invalid_chars_and_trailing_dots_or_spaces() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // Reserved device names (case-insensitive, with or without extension)
        for reserved in ["CON", "prn", "Aux.md", "NUL", "COM1.md", "lpt9", "Folder/CON.md"] {
            assert!(
                resolve_safe_vault_path(root, reserved).is_err(),
                "Expected reserved name '{reserved}' to be rejected"
            );
        }

        // Invalid characters
        for invalid in ["Note?.md", "A:B.md", "Star*.md", "Pipe|.md", "Quote\".md", "Angle<>.md"] {
            assert!(
                resolve_safe_vault_path(root, invalid).is_err(),
                "Expected invalid chars in '{invalid}' to be rejected"
            );
        }

        // Trailing spaces or dots (both on folder/note name and before .md)
        for trailing in ["TrailingDot.", "TrailingSpace ", "Note..md", "Note .md", "Folder./Note.md"] {
            assert!(
                resolve_safe_vault_path(root, trailing).is_err(),
                "Expected trailing space/dot in '{trailing}' to be rejected"
            );
        }

        // Valid names pass
        assert!(resolve_safe_vault_path(root, "Meeting Notes (2026-10-19).md").is_ok());
    }

    #[test]
    fn vault_crud_and_noop_save_preserve_files_and_ignore_obsidian() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // Create .obsidian folder that must be ignored
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::write(root.join(".obsidian/config.md"), "ignored").unwrap();

        // Create folder and notes
        let snap1 = create_vault_folder(root, "Daily").unwrap();
        assert_eq!(snap1.tree.len(), 1);
        assert_eq!(snap1.tree[0].name, "Daily");

        let (snap2, doc) = create_vault_note(root, "Daily/2026-10-18", "# Standup\nAll good").unwrap();
        assert_eq!(snap2.note_count, 1);
        assert_eq!(doc.rel_path, "Daily/2026-10-18.md");
        assert_eq!(doc.title, "2026-10-18");

        // Saving identical content must not modify the file (changed == false)
        let (_, changed_same) =
            save_vault_note(root, "Daily/2026-10-18.md", "# Standup\nAll good").unwrap();
        assert!(!changed_same);

        // Saving modified content writes atomically and reports changed == true
        let (updated_doc, changed_mod) =
            save_vault_note(root, "Daily/2026-10-18.md", "# Standup\nUpdated item").unwrap();
        assert!(changed_mod);
        assert_eq!(updated_doc.content, "# Standup\nUpdated item");

        // Rename note
        let renamed = rename_vault_entry(root, "Daily/2026-10-18.md", "Daily/2026-10-19").unwrap();
        assert_eq!(renamed.new_rel_path, "Daily/2026-10-19.md");
        assert!(!root.join("Daily/2026-10-18.md").exists());
        assert!(root.join("Daily/2026-10-19.md").exists());

        // Delete folder (moves to Windows Recycle Bin via trash crate)
        let snap_after_del = delete_vault_entry(root, "Daily").unwrap();
        assert_eq!(snap_after_del.note_count, 0);
        assert!(snap_after_del.tree.is_empty());
        // .obsidian was never touched
        assert!(root.join(".obsidian/config.md").exists());
    }

    #[test]
    fn recent_vaults_records_deduplicates_and_removes() {
        let temp = tempfile::tempdir().unwrap();
        let storage = temp.path().join("recent_vaults.json");
        let v1 = temp.path().join("VaultOne");
        let v2 = temp.path().join("VaultTwo");
        fs::create_dir_all(&v1).unwrap();
        fs::create_dir_all(&v2).unwrap();

        let list1 = record_recent_vault(&storage, &v1);
        assert_eq!(list1.len(), 1);

        let list2 = record_recent_vault(&storage, &v2);
        assert_eq!(list2.len(), 2);
        assert_eq!(list2[0].name, "VaultTwo");

        // Re-opening VaultOne moves it to index 0 without duplicating
        let list3 = record_recent_vault(&storage, &v1);
        assert_eq!(list3.len(), 2);
        assert_eq!(list3[0].name, "VaultOne");

        let list4 = remove_recent_vault(&storage, &v1.to_string_lossy());
        assert_eq!(list4.len(), 1);
        assert_eq!(list4[0].name, "VaultTwo");
    }
}
