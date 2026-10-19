# Changelog

All notable changes to Jotter are listed here, newest first. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

Roadmap Phase 2, first part.

### Added

- **Calculations.** A line ending in an arithmetic expression and `=` shows the result; `Tab` or a click inserts it. ```` ```calc ```` blocks show a result for each line, with variables, `sum`, `total`, and `prev`. HTML export and Print to PDF write the results after each line.
- **Properties form.** The note header edits YAML frontmatter as typed fields: text, list, number, checkbox, date, and date and time. Edits rewrite only the changed property and join the undo history. Live mode hides the raw YAML.
- **Outliner.** `Alt+↑`/`Alt+↓` move list items with their children, `Tab`/`Shift+Tab` nest and un-nest them, margin arrows fold them, and `Ctrl+.` zooms into one item. The command palette has zoom, fold all, and unfold all.
- **Cloud folder sync.** Jotter detects Dropbox, OneDrive, Google Drive, Box, and iCloud Drive vault folders, lists conflict copies in a Sync conflicts sidebar view and the status bar, and shows the detected client in Settings → Vault. The README describes the setup.

## [0.1.1] - 2026-10-19

### Fixed

- Failed saves keep dirty tabs, the window, and the vault open. Window close requests and tray Quit wait for successful saves, including edits made during an earlier save.
- Clipboard cuts and pastes cancel if their destination document changes while IPC is pending.
- Vault operations reject links and Windows junctions below the chosen root. Scans skip reparse points and directory cycles.
- Unicode note names no longer panic. Folder names containing `_` or `%` no longer affect unrelated index rows or edit history.
- External edits use a newer filesystem modification date without deleting in-app history.
- Image previews reset across vaults and attachment changes. The cache has a size limit and rejects stale responses.
- Rich blocks respect Markdown code and container context. Printing uses light diagram colors and excludes export-document styles from the app.

### Changed

- Batch index updates resolve links once per transaction. Rich-block reparsing and tag/statistics scans defer work while typing; selection-only changes reuse document statistics.
- Recycle Bin enumeration runs off the UI thread and refreshes independently of autosaves.
- Production bundles use minification and a Content Security Policy. Tantivy 0.27 removes the affected `lru` dependency version.

### Added

- Frontend regression tests, filesystem/index regressions, and an isolated WebView2 desktop verification script.

Roadmap Phase 1.

### Added

- **Math.** Live Preview renders `$…$` inline math and `$$…$$` display math with KaTeX. Placing the cursor in a block shows its source with a rendered preview below.
- **Mermaid diagrams.** ```` ```mermaid ```` blocks render as themed SVG through Pele. Invalid diagrams show the error message and line number.
- **Math and diagrams in export.** HTML export and Print to PDF render math and diagrams. HTML files use MathML, so they need no fonts or stylesheets.
- **Daily notes.** `Ctrl+Shift+D` and the command palette open or create today's note in `Daily/` from a template note. The folder and template path are set in Settings → Vault.
- **Recently deleted view.** A sidebar view lists the open vault's notes and folders in the Recycle Bin and restores them to their original paths.
- **Typo-tolerant search.** Full-text search matches words within one or two edits and ranks exact matches first. Highlighted snippets mark the near-match words.
- **Spell check.** The editor checks spelling against the operating system's languages. Toggle it in Settings → Editor.
- **Summon hotkey and tray icon.** An opt-in global hotkey (default `Ctrl+Alt+J`) shows or hides Jotter from any app. While it is on, Jotter stays in the tray, and closing the window hides it.
- **CSS snippets.** `.css` files in `%LOCALAPPDATA%\Jotter\snippets` load on top of the theme, each with its own toggle in the new Settings → System page.
- **License.** Jotter is licensed under MIT (`LICENSE-MIT`).

### Changed

- Closing the window saves pending edits before the window closes.
- HTML export renders the note on the frontend before saving, so `export_active_note_html` takes finished HTML instead of Markdown.
- The Markdown export renderer (comrak) enables its `math_dollars` extension.

## [0.1.0]

First release: Markdown vaults compatible with Obsidian, Notepad-faithful copy modes, Live Preview, wikilinks and backlinks, tags, full-text search, the quick switcher, the calendar view, focus modes, image paste, and HTML and PDF export.
