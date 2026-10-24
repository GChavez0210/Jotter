# Changelog

All notable changes to Jotter are listed here, newest first. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

Roadmap Phase 2.

### Removed

- Remove Mica and all app-level backdrop integration, including its setting, state, IPC commands, theme tokens, and direct dependency. Use opaque window surfaces on every desktop.

### Added

- **Pinned notes.** Right-click a note and choose **Pin to top** to keep it in a Pinned section above the file tree. Pins keep the order you made them, follow a note through a rename or a move, and survive a trip to the Recycle Bin and back. They are stored per vault in the app data folder, so they never appear in the vault or travel to another computer through a sync folder.
- **Narrow windows.** The window now resizes down to 480 px wide, so Jotter fits beside another app. Below 608 px the sidebar steps aside to keep the text column readable, and showing it again widens the window by just enough to fit both. Widening the window brings the sidebar back on its own, unless you collapsed it yourself. Below 600 px the title bar trades its text labels for icons so the window controls stay reachable.
- **Single instance.** Launching Jotter while it already runs brings the open window to the front, including from the tray or a minimized state, instead of starting a second copy.
- **Close button and tray settings.** **Settings → System → Keep Jotter in the tray** controls the tray icon, and **Closing the window** chooses between quitting and hiding to the tray. The summon hotkey no longer forces either one. Choosing to hide turns the tray icon on, so the window always has a way back. The tray menu's Quit still exits after saving.
- **Line numbers.** **Settings → Editor → Line numbers**, or the command palette, shows a number beside each line. Numbers line up with the first row of headings, code blocks, and tables, and the text column stays centered.
- **Spanish interface.** **Settings → Appearance → Language** switches menus, dialogs, and messages between English and Spanish without restarting. **System**, the default, follows the OS language and falls back to English. Dates, month names, the tray menu, and native save dialogs follow the chosen language. Error text from the Rust backend stays in English.
- **Bring-your-own AI.** Settings → AI connects an OpenAI-compatible endpoint, Anthropic, or a local Ollama model. It is off by default and needs no Jotter account. The command palette and the selection toolbar open an AI panel that summarizes, rewrites, or translates the selection or the whole note. The panel previews the result, then replaces the text or inserts the result below it as one undo step. API keys stay in the OS credential store and go only to the address they were saved for. Code, links, math, tags, and frontmatter pass through unchanged. A **Writing style** box holds your own style instructions, prefilled with an example, and the **Polish** rewrite applies them.

- Track pending desktop verification and Mac release work in the roadmap. Expand Settings → System → CSS snippets with setup steps, a working CSS example, scope, and undo instructions.

- Prepare desktop portability: native cross-platform dialogs, persistent OS data paths, Mac shortcut hints, opaque native Mac frame, app icon, and macOS CI. Preserve shared-vault filename safety. Fix spelling reload/persistence and case-sensitive rename protection.

- **Calculations.** A line ending in an arithmetic expression and `=` shows the result; `Tab` or a click inserts it. ```` ```calc ```` blocks show a result for each line, with variables, `sum`, `total`, and `prev`. HTML export and Print to PDF write the results after each line.
- **Properties form.** The note header edits YAML frontmatter as typed fields: text, list, number, checkbox, date, and date and time. Edits rewrite only the changed property and join the undo history. Live mode hides the raw YAML.
- **Outliner.** `Alt+↑`/`Alt+↓` move list items with their children, `Tab`/`Shift+Tab` nest and un-nest them, margin arrows fold them, and `Ctrl+.` zooms into one item. The command palette has zoom, fold all, and unfold all.
- **Cloud folder sync.** Jotter detects Dropbox, OneDrive, Google Drive, Box, and iCloud Drive vault folders, lists conflict copies in a Sync conflicts sidebar view and the status bar, and shows the detected client in Settings → Vault. The README describes the setup.
- **Task due dates and calendar file.** Checklist items with `📅 2026-11-01`, `due:2026-11-01`, or `[due:: 2026-11-01]` go into the index. Live mode shows them as chips that turn red when overdue. Settings → Vault picks an `.ics` file that Jotter keeps current with open tasks.
- **Tags and links view.** The Notes sidebar can list notes by nested tag, with each note expandable to the notes it links to.
- **Spelling dictionaries.** Jotter checks spelling with bundled Hunspell dictionaries for English (US and UK), Spanish, French, German, Portuguese (Brazil), and Italian. Each vault picks its languages in Settings → Editor, and a note's `lang` property overrides them. Right-click an underlined word for suggestions or to add it to the personal dictionary. User `.aff`/`.dic` files add more languages. The previous WebView checker remains available as the System option.
- **Home page.** With no note open, a page of note cards shows recently opened, edited, and created notes, the most linked notes, and a daily Rediscover pick. Sections are customizable per vault.

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
