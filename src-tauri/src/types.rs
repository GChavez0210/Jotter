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
