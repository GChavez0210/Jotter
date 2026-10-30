//! Platform paths and portable export names.
use std::path::{Path, PathBuf};

/// Keep the existing Windows location so upgrades retain recent vaults and dictionaries.
/// Other desktops use the OS data directory; persistent data must never fall back to temp.
pub fn app_data_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("LOCALAPPDATA").filter(|p| !p.is_empty()).map(PathBuf::from)
        .or_else(dirs::data_local_dir);
    #[cfg(not(target_os = "windows"))]
    let root = dirs::data_local_dir();
    root
        .map(|dir| dir.join("Jotter"))
        .ok_or_else(|| "The operating system did not provide an application data directory".into())
}

/// Stable 64-bit FNV-1a hash.
fn fnv1a_64(input: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Names the app data files that belong to one vault: its index cache and its pinned notes.
/// Windows resolves one vault through several spellings, so the key folds case and slashes.
pub fn vault_key(vault_root: &Path) -> String {
    let key = vault_root.to_string_lossy().into_owned();
    #[cfg(target_os = "windows")]
    let key = {
        let key = key.to_lowercase().replace('\\', "/");
        if let Some(unc) = key.strip_prefix("//?/unc/") {
            format!("//{unc}")
        } else {
            key.strip_prefix("//?/").unwrap_or(&key).to_string()
        }
    };
    format!("{:016x}", fnv1a_64(key.trim_end_matches('/')))
}

/// Names the folder holding one note's version history. Windows and macOS open `Note.md` and
/// `note.md` as one file, so the key folds case there, as the vault does.
pub fn note_key(rel_path: &str) -> String {
    let key = rel_path.replace('\\', "/");
    let key = if cfg!(target_os = "linux") { key } else { key.to_lowercase() };
    format!("{:016x}", fnv1a_64(&key))
}

pub fn export_file_name(title: &str, extension: &str) -> String {
    let clean: String = title.chars().map(|c| {
        if c.is_control() || "<>:\"/\\|?*".contains(c) { '-' } else { c }
    }).collect();
    let clean = clean.trim().trim_end_matches('.');
    let clean = if clean.is_empty() { "Untitled" } else { clean };
    if clean.to_lowercase().ends_with(&format!(".{extension}")) {
        clean.into()
    } else {
        format!("{clean}.{extension}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_names_are_portable_and_keep_the_extension() {
        assert_eq!(export_file_name("Note: a/b", "html"), "Note- a-b.html");
        assert_eq!(export_file_name("Report.HTML", "html"), "Report.HTML");
        assert_eq!(export_file_name(" . ", "ics"), "Untitled.ics");
        assert_eq!(export_file_name("line\nbreak", "ics"), "line-break.ics");
    }

    #[test]
    #[cfg(windows)]
    fn extended_windows_paths_share_the_existing_vault_key() {
        assert_eq!(vault_key(Path::new(r"C:\Notes")), vault_key(Path::new(r"\\?\C:\Notes")));
        assert_eq!(vault_key(Path::new(r"\\server\share\Notes")), vault_key(Path::new(r"\\?\UNC\server\share\Notes")));
    }

}
