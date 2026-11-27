//! Which notes AI features may read. Nothing is shared until the user selects it.
//!
//! The selection lives in the app data folder, outside the vault, so an assistant that edits
//! notes cannot change it. A note whose frontmatter says `ai: false` is blocked whatever the
//! selection says, so moving a private note into a shared folder does not expose it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The folder assistants work in. Once the user sets it up, the MCP server reads it and
/// writes only inside it.
pub const AI_FOLDER: &str = "AI";

pub fn in_ai_folder(path: &str) -> bool {
    path.strip_prefix(AI_FOLDER).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Consent belongs to one directory, rather than whatever later takes its path.
fn ai_folder_identity(root: &Path) -> Result<String, String> {
    if disk_spelling(root, AI_FOLDER).as_deref() != Some(AI_FOLDER) {
        return Err("The AI folder is missing or was renamed.".into());
    }
    let (path, _) = crate::vault::resolve_safe_vault_path(root, AI_FOLDER)?;
    if !path.is_dir() {
        return Err("The AI folder is missing.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(&path).map_err(|e| format!("Cannot inspect the AI folder: {e}"))?;
        let created = metadata.created().ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|time| time.as_nanos()).unwrap_or(0);
        Ok(format!("{}:{}:{created}", metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS};
        let file = fs::OpenOptions::new().read(true).custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
            .open(&path).map_err(|e| format!("Cannot inspect the AI folder: {e}"))?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // The file owns this handle and keeps it alive for the metadata call.
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }
            .map_err(|e| format!("Cannot identify the AI folder: {e}"))?;
        Ok(format!("{}:{}:{}:{}:{}", info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow,
            info.ftCreationTime.dwHighDateTime, info.ftCreationTime.dwLowDateTime))
    }
}

/// Whether the path in the AI folder tells assistants how to behave: the preferences, the
/// instruction notes, and any `AGENTS.md` or `CLAUDE.md`. Case is ignored, so another
/// spelling cannot slip past.
fn is_instructions(path: &str) -> bool {
    let path = path.to_lowercase();
    let Some(rest) = path.strip_prefix("ai/") else {
        return false;
    };
    rest.split('/').any(|part| part == "instructions")
        || matches!(rest.rsplit('/').next(), Some("agents.md" | "claude.md" | "preferences.md"))
}

/// Whether the note's frontmatter sets `ai` to a false value. It reads more leniently than the
/// property editor: a malformed or unclosed block still blocks the note.
pub fn blocks_ai(content: &str) -> bool {
    let mut lines = content.strip_prefix('\u{feff}').unwrap_or(content).lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return false;
    }
    let lines: Vec<_> = lines.take_while(|line| !matches!(line.trim_end(), "---" | "...")).collect();
    let indent = lines.iter()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .map(|line| line.len() - line.trim_start_matches([' ', '\t']).len())
        .min().unwrap_or(0);
    // Set after an `ai:` line with no value, whose value may sit on the next indented line.
    let mut pending = false;
    for raw in lines {
        let line = raw.get(indent..).unwrap_or(raw);
        let trimmed = line.trim_end();
        if trimmed == "---" || trimmed == "..." {
            break;
        }
        if pending {
            if trimmed.is_empty() || trimmed.trim_start().starts_with('#') {
                continue;
            }
            if line.starts_with([' ', '\t']) && is_false(line) {
                return true;
            }
            pending = false;
        }
        if crate::frontmatter::key_of(line).is_some_and(|key| key.eq_ignore_ascii_case("ai")) {
            let value = line.split_once(':').map(|(_, value)| value).unwrap_or_default();
            if value.split('#').next().unwrap_or_default().trim().is_empty() {
                pending = true;
            } else if is_false(value) {
                return true;
            }
        }
    }
    false
}

fn is_false(value: &str) -> bool {
    let value = value.split('#').next().unwrap_or_default();
    let value = value.trim().trim_matches(['"', '\'']).trim().to_ascii_lowercase();
    matches!(value.as_str(), "false" | "no" | "off" | "n" | "0")
}

/// `rel_path` spelled as the folders and files on disk spell it, so that a request for
/// `private/note.md` is judged by the rules for `Private/Note.md` on a case-insensitive disk.
/// Parts that do not exist yet stay as given. `None` for another alias of an existing entry,
/// such as a Windows short name.
pub fn disk_spelling(root: &Path, rel_path: &str) -> Option<String> {
    let mut current = root.to_path_buf();
    let mut parts = Vec::new();
    let mut missing = false;
    for part in rel_path.split('/') {
        missing = missing || fs::symlink_metadata(current.join(part)).is_err();
        let spelled = if missing {
            part.to_string()
        } else {
            let names: Vec<String> = fs::read_dir(&current)
                .ok()?
                .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
                .collect();
            let wanted = part.to_lowercase();
            names
                .iter()
                .find(|name| *name == part)
                .or_else(|| names.iter().find(|name| name.to_lowercase() == wanted))?
                .clone()
        };
        current.push(&spelled);
        parts.push(spelled);
    }
    Some(parts.join("/"))
}

fn is_within(path: &str, folder: &str) -> bool {
    folder.is_empty() || path.strip_prefix(folder).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The user's selection for one vault: vault-relative note and folder paths, each shared or
/// not. The empty path is the whole vault. The nearest rule above a note decides.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessRules {
    #[serde(default)]
    rules: BTreeMap<String, bool>,
    /// The user set up the AI folder, so assistants may use it.
    #[serde(default)]
    pub ai_folder: bool,
    #[serde(default)]
    ai_folder_identity: Option<String>,
    /// Assistants may also change the notes that instruct them.
    #[serde(default)]
    pub edit_instructions: bool,
}

impl AccessRules {
    pub fn file(app_data_dir: &Path, vault_root: &Path) -> PathBuf {
        app_data_dir.join("ai-access").join(format!("{}.json", crate::platform::vault_key(vault_root)))
    }

    /// A vault with no saved selection shares nothing. An unreadable selection is an error, so
    /// callers share nothing rather than guess.
    pub fn load(app_data_dir: &Path, vault_root: &Path) -> Result<Self, String> {
        let file = Self::file(app_data_dir, vault_root);
        match fs::read(&file) {
            Ok(bytes) => {
                let mut rules: Self = serde_json::from_slice(&bytes).map_err(|e| {
                    format!("Cannot read the AI note selection {}: {e}. Delete the file and select the notes again.", file.display())
                })?;
                if rules.ai_folder && (rules.ai_folder_identity.is_none()
                    || ai_folder_identity(vault_root).ok().as_ref() != rules.ai_folder_identity.as_ref()) {
                    rules.ai_folder = false;
                    rules.edit_instructions = false;
                }
                Ok(rules)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("Cannot read the AI note selection {}: {e}", file.display())),
        }
    }

    /// Loads the selection, applies `change`, and saves it if it differs. Callers hold the
    /// vault write lock.
    pub fn update(app_data_dir: &Path, vault_root: &Path, change: impl FnOnce(&mut Self)) -> Result<Self, String> {
        let current = Self::load(app_data_dir, vault_root)?;
        let mut next = current.clone();
        change(&mut next);
        if next.ai_folder && !current.ai_folder {
            next.ai_folder_identity = Some(ai_folder_identity(vault_root)?);
        }
        if next != current {
            let bytes = serde_json::to_vec_pretty(&next).map_err(|e| format!("Cannot encode the AI note selection: {e}"))?;
            crate::fs_atomic::atomic_write_bytes(&Self::file(app_data_dir, vault_root), &bytes)?;
        }
        Ok(next)
    }

    pub fn allows(&self, path: &str) -> bool {
        let mut current = path;
        loop {
            if let Some(allowed) = self.rules.get(current) {
                return *allowed;
            }
            if current.is_empty() {
                return false;
            }
            current = current.rsplit_once('/').map_or("", |(parent, _)| parent);
        }
    }

    /// Whether an MCP assistant may read the path: the user selected it, or it is in the AI
    /// folder.
    pub fn can_read(&self, path: &str) -> bool {
        (self.ai_folder && in_ai_folder(path)) || self.allows(path)
    }

    /// Why an MCP assistant may not change the path, if it may not.
    pub fn check_write(&self, path: &str) -> Result<(), String> {
        if !self.ai_folder {
            Err("The AI folder is not set up, so assistants cannot change notes. Ask the user to set it up in Jotter under Settings > AI.".into())
        } else if !in_ai_folder(path) {
            Err(format!("Assistants can change only notes in the {AI_FOLDER} folder, and '{path}' is outside it."))
        } else if !self.edit_instructions && is_instructions(path) {
            Err(format!("'{path}' holds instructions for assistants. Show the user the new text, or ask them to allow these edits in Jotter under Settings > AI."))
        } else {
            Ok(())
        }
    }

    /// Shares or unshares a note or folder, replacing the choices made inside it.
    pub fn set(&mut self, path: &str, allowed: bool) {
        self.rules.retain(|rule, _| !is_within(rule, path));
        if self.allows(path) != allowed {
            self.rules.insert(path.to_string(), allowed);
        }
    }

    /// Moves the rules of a renamed note or folder with it.
    pub fn rename(&mut self, from: &str, to: &str) {
        if from.is_empty() || to.is_empty() {
            return;
        }
        // The setup covers the folder the user agreed to, not one later moved to its name.
        if from == AI_FOLDER {
            self.ai_folder = false;
            self.edit_instructions = false;
        }
        let moved: Vec<(String, bool)> = self
            .rules
            .iter()
            .filter(|(rule, _)| is_within(rule, from))
            .map(|(rule, allowed)| (format!("{to}{}", &rule[from.len()..]), *allowed))
            .collect();
        self.rules.retain(|rule, _| !is_within(rule, from));
        // A leftover share at the destination must not apply to what arrives there.
        self.forget(to);
        self.rules.extend(moved);
    }

    /// Drops the shares of a deleted note or folder, so a later one with the same name starts
    /// private. Choices to hide it stay, in case it comes back from the trash.
    pub fn forget(&mut self, path: &str) {
        if path == AI_FOLDER {
            self.ai_folder = false;
            self.edit_instructions = false;
        }
        self.rules.retain(|rule, allowed| !(*allowed && is_within(rule, path)));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderAccess {
    pub path: String,
    /// Whether a note added to this folder starts out shared.
    pub shared: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteAccess {
    pub path: String,
    pub shared: bool,
    /// The note says `ai: false`.
    pub blocked: bool,
}

/// What the note picker shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessOverview {
    pub vault_shared: bool,
    pub folders: Vec<FolderAccess>,
    pub notes: Vec<NoteAccess>,
}

pub fn overview(vault_root: &Path, rules: &AccessRules) -> Result<AccessOverview, String> {
    let mut folders = BTreeSet::new();
    let mut notes = Vec::new();
    for file in crate::index::scan_dir_files_from_disk(vault_root, vault_root)? {
        let mut parent = file.path.as_str();
        while let Some((folder, _)) = parent.rsplit_once('/') {
            folders.insert(folder.to_string());
            parent = folder;
        }
        let blocked = blocks_ai(&file.content);
        notes.push(NoteAccess { shared: !blocked && rules.allows(&file.path), blocked, path: file.path });
    }
    notes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(AccessOverview {
        vault_shared: rules.allows(""),
        folders: folders.into_iter().map(|path| FolderAccess { shared: rules.allows(&path), path }).collect(),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_false_blocks_a_note_in_every_spelling() {
        for note in [
            "---\nai: false\n---\nBody",
            "---\r\ntitle: A\r\nAI: False # private\r\n---\r\n",
            "\u{feff}---\n\"ai\": 'no'\n---\n",
            "---\nai:\n  off\n---\n",
            "---\nai: false\nnever closed",
            "---\n  ai: FALSE\n  title: Private\n---\n",
            "---\nai:\n  # private\n  off\n---\n",
            "---\n: not yaml [\nai: false\n---\n",
        ] {
            assert!(blocks_ai(note), "{note:?}");
        }
        for note in [
            "Body\nai: false",
            "---\nai: true\n---\n",
            "---\ntitle: A\n---\nai: false\n",
            "---\nnested:\n  ai: false\n---\n",
            "---\nmail: false\n---\n",
        ] {
            assert!(!blocks_ai(note), "{note:?}");
        }
    }

    #[test]
    fn nothing_is_shared_until_selected_and_the_nearest_rule_wins() {
        let mut rules = AccessRules::default();
        assert!(!rules.allows("A.md"));
        rules.set("Projects", true);
        rules.set("Projects/Secret.md", false);
        assert!(rules.allows("Projects/Plan.md"));
        assert!(rules.allows("Projects/Deep/New.md"));
        assert!(!rules.allows("Projects/Secret.md"));
        assert!(!rules.allows("ProjectsOld/Plan.md"));
        // Selecting the folder again replaces the choices inside it.
        rules.set("Projects", true);
        assert!(rules.allows("Projects/Secret.md"));
        rules.set("", false);
        assert_eq!(rules, AccessRules::default());
    }

    #[test]
    fn rules_follow_renames_and_deleted_shares_are_forgotten() {
        let mut rules = AccessRules::default();
        rules.set("Work", true);
        rules.set("Work/Secret.md", false);
        rules.set("Old.md", true);
        rules.rename("Work", "Job");
        assert!(rules.allows("Job/Plan.md") && !rules.allows("Job/Secret.md"));
        assert!(!rules.allows("Work/Plan.md"));
        // A hidden note moved over a leftover share stays hidden.
        rules.rename("Job/Secret.md", "Old.md");
        assert!(!rules.allows("Old.md"));
        rules.forget("Job");
        assert!(!rules.allows("Job/Plan.md"));
    }

    #[test]
    fn the_selection_is_saved_per_vault_and_a_damaged_file_is_an_error() {
        let vault = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        assert!(!AccessRules::load(data.path(), vault.path()).unwrap().allows("A.md"));
        AccessRules::update(data.path(), vault.path(), |rules| rules.set("", true)).unwrap();
        assert!(AccessRules::load(data.path(), vault.path()).unwrap().allows("A.md"));
        fs::write(AccessRules::file(data.path(), vault.path()), "{").unwrap();
        assert!(AccessRules::load(data.path(), vault.path()).is_err());
    }

    #[test]
    fn assistants_write_only_in_the_set_up_folder_and_not_in_its_instructions() {
        let mut rules = AccessRules::default();
        assert!(!rules.can_read("AI/Context.md"));
        assert!(rules.check_write("AI/Context.md").unwrap_err().contains("not set up"));
        rules.ai_folder = true;
        rules.set("", true);
        assert!(rules.can_read("AI/Context.md"));
        assert!(rules.check_write("AI/Context.md").is_ok());
        assert!(rules.check_write("AI/Workflows/Report/Workflow.md").is_ok());
        for path in ["Plan.md", "AIX/a.md", "ai-notes.md"] {
            assert!(rules.check_write(path).unwrap_err().contains("outside"), "{path}");
        }
        let instructions = [
            "AI/AGENTS.md", "AI/agents.md", "AI/Preferences.md", "AI/Instructions",
            "AI/instructions/Tone.md", "AI/Workflows/AGENTS.md", "AI/Outputs/CLAUDE.md",
            "AI/Outputs/Preferences.md", "AI/Workflows/Job/InStrUcTiOnS/Style.md",
        ];
        for path in instructions {
            assert!(rules.check_write(path).unwrap_err().contains("instructions"), "{path}");
        }
        rules.edit_instructions = true;
        assert!(instructions.iter().all(|path| rules.check_write(path).is_ok()));
        // Moving the folder away withdraws the setup, even if another takes its name.
        rules.rename("AI", "Old AI");
        assert!(!rules.ai_folder);
        rules.ai_folder = true;
        rules.rename("Private", "AI");
        assert!(!rules.ai_folder);
    }

    #[test]
    fn disk_spelling_matches_the_files_on_disk() {
        let vault = tempfile::tempdir().unwrap();
        fs::create_dir(vault.path().join("Private")).unwrap();
        fs::write(vault.path().join("Private/Note.md"), "x").unwrap();
        assert_eq!(disk_spelling(vault.path(), "Private/New.md").unwrap(), "Private/New.md");
        // Only a case-insensitive disk resolves the other spelling to the existing entry.
        if vault.path().join("private").exists() {
            assert_eq!(disk_spelling(vault.path(), "private/note.md").unwrap(), "Private/Note.md");
        }
    }

    #[test]
    fn the_overview_lists_folders_and_marks_blocked_notes() {
        let vault = tempfile::tempdir().unwrap();
        fs::create_dir_all(vault.path().join("A/B")).unwrap();
        fs::write(vault.path().join("A/B/Open.md"), "x").unwrap();
        fs::write(vault.path().join("A/Shut.md"), "---\nai: false\n---\n").unwrap();
        let mut rules = AccessRules::default();
        rules.set("A", true);
        let view = overview(vault.path(), &rules).unwrap();
        let folders: Vec<_> = view.folders.iter().map(|f| (f.path.as_str(), f.shared)).collect();
        assert_eq!(folders, [("A", true), ("A/B", true)]);
        let notes: Vec<_> = view.notes.iter().map(|n| (n.shared, n.blocked)).collect();
        assert_eq!(notes, [(true, false), (false, true)]);
    }
}
