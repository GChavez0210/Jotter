//! Platform paths and portable export names.
use std::path::PathBuf;

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
}
