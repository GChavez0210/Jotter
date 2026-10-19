use serde::{Deserialize, Serialize};

/// Copy mode selectable by the user in settings / toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CopyMode {
    /// Default: raw Markdown source with exact line breaks preserved.
    #[default]
    MarkdownSource,
    /// Opt-in clean text (Markdown syntax stripped), text/plain only, line breaks preserved.
    Plain,
    /// Rich HTML (via comrak with <br> line breaks) plus plain-text alternative.
    Rich,
}

/// IPC request payload for copying selected text to the system clipboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyRequest {
    pub text: String,
    pub mode: CopyMode,
}

/// Prepared clipboard formats (`text/plain` with `\n` and `text/html` with `<br>`)
/// returned to the frontend for inspection and status feedback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardPayload {
    pub plain: String,
    pub html: String,
    pub mode: CopyMode,
    pub line_count: usize,
    pub char_count: usize,
}

/// Window backdrop state reported to the frontend so it can toggle solid-color fallback
/// when Windows 11 Mica is unavailable or transparency effects are disabled.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackdropState {
    pub mica_applied: bool,
    pub backdrop_kind: String,
    pub dark_mode: Option<bool>,
}

/// Parsed Obsidian-compatible `[[wiki link]]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiLink {
    pub raw: String,
    pub target: String,
    pub heading: Option<String>,
    pub alias: Option<String>,
}

/// Which timestamp column to aggregate by in the Calendar view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CalendarDateField {
    #[default]
    Modified,
    Created,
}

/// Per-day summary count for a calendar month grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDaySummary {
    /// ISO date string `YYYY-MM-DD`.
    pub date: String,
    /// Number of notes touched on that date.
    pub count: usize,
}

/// Note entry returned when clicking a day in the calendar view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarNoteEntry {
    pub path: String,
    pub title: String,
    pub modified_ms: i64,
    pub created_ms: i64,
    pub preview: String,
}

/// Node type in the vault file tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultNodeKind {
    File,
    Folder,
}

/// A folder or `.md` note in the vault file tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultNode {
    pub name: String,
    pub rel_path: String,
    pub kind: VaultNodeKind,
    pub modified_ms: i64,
    pub created_ms: i64,
    pub children: Vec<VaultNode>,
}

/// Snapshot of the currently opened vault directory and its file tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultSnapshot {
    pub root_path: String,
    pub name: String,
    pub tree: Vec<VaultNode>,
    pub note_count: usize,
}

/// Loaded `.md` note document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDocument {
    pub rel_path: String,
    pub title: String,
    pub content: String,
    pub modified_ms: i64,
    pub created_ms: i64,
}

/// Entry in the user's recent vaults list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentVault {
    pub path: String,
    pub name: String,
    pub last_opened_ms: i64,
}

/// Result returned when renaming a file or folder inside the vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameEntryResult {
    pub snapshot: VaultSnapshot,
    pub old_rel_path: String,
    pub new_rel_path: String,
}

/// Payload emitted via Tauri event `"vault-fs-event"` when external filesystem changes are detected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultFsEvent {
    pub changed_paths: Vec<String>,
}
