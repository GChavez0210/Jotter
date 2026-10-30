# Changelog

All notable changes to Jotter are listed here, newest first. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.5.2] - 2026-10-29

### Added

- **Automatic updates.** Jotter checks GitHub for a newer release shortly after it starts and once a day, and an **Update** button appears in the status bar when one exists. It opens the release notes with **Install and restart**, which saves your open notes, downloads the update, verifies its signature against the key built into the app, installs it, and restarts. Nothing installs without your confirmation. **Settings → About** turns the daily check off and checks on demand, and the command palette has **Check for Updates**. A Linux `.deb` install cannot replace itself and points to the release page instead. The AppImage and the Windows installers update in place.
- **Calendar sync (CalDAV).** **Settings → Task calendar** can push open tasks with due dates to a CalDAV calendar such as Nextcloud, Fastmail, iCloud, or Radicale. Jotter sends changes one way, never reads the calendar, and deletes the events it created when a task is completed or removed. Use a calendar that holds only Jotter's events. The password stays in the system credential store.
- **Scheduled vault backups.** **Settings** can zip the whole vault into a folder you choose every 6 hours, daily, or weekly, and keep the newest copies you set. **Back up now** runs one on demand, and Settings shows the time of the last backup. Backups continue while Jotter sits in the tray. Each vault gets its own backup subfolder, and repeated copies use unique names. Existing flat backups stay untouched. Symlinks are skipped, and so is the backup folder when it sits inside the vault.
- **`jotter` command-line tool and MCP server.** Search, read, create, append to, rename, and delete notes, and read backlinks, tags, tasks, vault health, and version history, from a terminal or an AI agent, with JSON answers. `jotter mcp` serves the same operations to MCP clients. Writes keep the replaced text in the note's version history, deletes go to the Recycle Bin, and `--read-only` refuses every write. See the README for build steps.
- Release notes. A GitHub release's body is now the version's section of this changelog, so the same text appears on GitHub and inside the update dialog.

### Fixed

- Pause autosave when a dirty note changes externally until the user chooses which copy to keep. Keeping local edits resumes saving; reloading preserves the external copy.
- Keep CLI version history in the same vault folder as the app, including Windows short paths and macOS path aliases. Carry history through note and folder renames, and rewrite links to moved notes and attachments.
- Separate the desktop and CLI executables so workspace builds cannot overwrite either program.
- Isolate backups by vault, use unique temporary and archive names, and serialize manual and scheduled runs. Keep the displayed backup status current.
- Stop CalDAV deletes after an authorization failure and retain unsent events for retry.
- Serialize release uploads to preserve every platform in the update feed. Install only the update version confirmed in the dialog, handle Escape and shortcuts consistently, and translate updater errors.

## [0.5.1] - 2026-10-27

### Added

- **Split view.** **Ctrl+Shift+\\**, the title bar's split button, or **Open on the right** in a note's context menu puts a second note beside the first one. Each pane keeps its own cursor, scroll position and undo history, and the pane you last typed in takes the next note you open. Drag the bar between them to resize, or double-click it to even them out. **Show Rendered Preview Beside the Note** in the command palette shows the open note as it will export instead of a second note. A window too narrow for two text columns hides the pane until it is wide enough again.
- **Accessibility pass.** An axe audit runs over the app's main screens in every theme as part of the interface checks, and the findings are fixed: the editor's text box carries its own name for a screen reader, a tab is one control instead of a control inside a control, small buttons meet the 24 px target size, muted text meets AA contrast in light and dark, and a dialog keeps Tab inside it until it closes. The sidebar's edge and the split view's divider now move with the arrow keys, and reset with Home or a double-click, so neither needs a drag.
- **Linux builds.** A tagged release now ships an x86_64 AppImage and a `.deb` beside the Windows installers, and CI runs the Rust tests on Linux. The desktop draws the window frame, so Jotter leaves the title bar to it. The clipboard enables native Wayland data-control support with an X11 fallback; native platform testing remains pending. Global shortcuts need X11: Wayland lets no application claim one, so the summon hotkey and quick capture say so instead of failing quietly, and `GDK_BACKEND=x11` makes them work inside a Wayland session.
- **Note version history.** Jotter periodically keeps the previous saved text of a note so you can recover from unwanted edits. **Version history** in a note's header, or the command palette, opens a timeline of stored versions with what restoring one would change, and restores it in a click. Restoring keeps the text it replaces as another version, so it can be undone the same way. Versions live in the app data folder, never in the vault, and are thinned as they age: everything from the last day, one a day for a month, one a week for a year. **Settings → Vault → Keep version history** turns it off.

### Fixed

- Keep each note in one editor pane, route note actions to the focused pane, retain heading targets, and show external-change controls for either note.
- Validate vault paths before reading or snapshotting history. Abort restoration when pending edits cannot be saved or the current text cannot be preserved.
- Preserve history on rename, including timestamp collisions, and give missing notes a full retention period from their first detected absence.
- Keep printing, word counts, shortcuts, and keyboard tab closing consistent with the visible note. Stabilize accessibility audits across theme changes.

## [0.5.0] - 2026-10-24

Windows builds now ship for both x64 and ARM64.

### Changed

- Default note names use local time instead of UTC, so a note made in the evening no longer carries tomorrow's date.
- Each window may call only the backend commands it needs. The quick capture window can read settings and hide itself, and nothing else.

### Removed

- Remove Mica and all app-level backdrop integration, including its setting, state, IPC commands, theme tokens, and direct dependency. Use opaque window surfaces on every desktop.

### Added

- **Callouts and footnotes.** Obsidian callouts (`> [!note]`, `> [!warning]-`, and the other 11 types and their aliases) render as colored boxes in Live Preview and in HTML and PDF export, with custom titles and a fold chevron for `+` and `-` callouts. Footnote references show as numbered superscripts that jump to their definition, numbered the same way as in export. A `-` callout starts closed only in HTML export; the editor keeps every callout open so it stays editable.
- **Templates.** Any note in the templates folder (**Settings → Vault**, default `Templates`) can start a new note from the command palette or a folder's right-click menu. `{{date}}`, `{{time}}`, `{{title}}`, and `{{prompt:Label}}` fill in, and each prompt gets its own field.
- **Vault health report.** **Check vault health** in the command palette lists broken links, attachments no note uses, empty notes, and duplicate titles. Broken wiki links offer to create the missing note, and unused files and empty notes move to the Recycle Bin after a confirmation. Notes in the templates folder are left out.
- **Quick capture.** A global hotkey (default `Ctrl+Alt+N`, set in **Settings → System**) opens a small window that appends text to an inbox note without bringing up the main window. When the inbox note is open, the entry goes into the editor, so unsaved typing is kept.
- **Update links on rename.** Renaming or moving a note that other notes link to asks whether to rewrite those `[[wiki links]]` and Markdown links. Aliases, headings, and embeds are kept, and a moved note's own relative links follow it to the new folder. **Undo last link update** in the command palette puts the links back.
- **Right panel.** The calendar and Recently deleted moved from the sidebar to the right panel, and the editor gained find and replace.
- **Settings file.** Settings live in the app data folder, and **Settings → System** exports and imports them, leaving out this computer's own entries such as the last vault.
- **Task reminders.** Jotter shows a notification when a task comes due, even while the window sits in the tray.
- **Outline panel.** The note panel on the right lists the open note's headings above its backlinks. Click one to jump to it, and the heading your cursor sits in stays highlighted as you move through the note. Headings inside code fences and YAML frontmatter are left out, and indentation is relative to the note's own shallowest heading, so a note written entirely in `##` does not start indented.
- **Pinned notes.** Right-click a note and choose **Pin to top** to keep it in a Pinned section above the file tree. Pins keep the order you made them, follow a note through a rename or a move, and survive a trip to the Recycle Bin and back. They are stored per vault in the app data folder, so they never appear in the vault or travel to another computer through a sync folder.
- **Narrow windows.** The window now resizes down to 480 px wide, so Jotter fits beside another app. Below 608 px the sidebar steps aside to keep the text column readable, and showing it again widens the window by just enough to fit both. Widening the window brings the sidebar back on its own, unless you collapsed it yourself. Below 600 px the title bar trades its text labels for icons so the window controls stay reachable. The note panel on the right yields its width before the sidebar does, so a narrowing window keeps the file tree longest.
- **Single instance.** Launching Jotter while it already runs brings the open window to the front, including from the tray or a minimized state, instead of starting a second copy.
- **Close button and tray settings.** **Settings → System → Keep Jotter in the tray** controls the tray icon, and **Closing the window** chooses between quitting and hiding to the tray. The summon hotkey no longer forces either one. Choosing to hide turns the tray icon on, so the window always has a way back. The tray menu's Quit still exits after saving.
- **Line numbers.** **Settings → Editor → Line numbers**, or the command palette, shows a number beside each line. Numbers line up with the first row of headings, code blocks, and tables, and the text column stays centered.
- **Spanish interface.** **Settings → Appearance → Language** switches menus, dialogs, and messages between English and Spanish without restarting. **System**, the default, follows the OS language and falls back to English. Dates, month names, the tray menu, and native save dialogs follow the chosen language. Error text from the Rust backend stays in English.
- **Bring-your-own AI.** Settings → AI connects an OpenAI-compatible endpoint, Anthropic, or a local Ollama model. It is off by default and needs no Jotter account. The command palette and the selection toolbar open an AI panel that summarizes, rewrites, or translates the selection or the whole note. The panel previews the result, then replaces the text or inserts the result below it as one undo step. API keys stay in the OS credential store and go only to the address they were saved for. Code, links, math, tags, and frontmatter pass through unchanged. A **Writing style** box holds your own style instructions, prefilled with an example, and the **Polish** rewrite applies them.

- Expand Settings → System → CSS snippets with setup steps, a working CSS example, scope, and undo instructions.

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
- Closing the window saves pending edits before the window closes.
- HTML export renders the note on the frontend before saving, so `export_active_note_html` takes finished HTML instead of Markdown.
- The Markdown export renderer (comrak) enables its `math_dollars` extension.

### Added

- Frontend regression tests, filesystem/index regressions, and an isolated WebView2 desktop verification script.
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

## [0.1.0]

First release: Markdown vaults compatible with Obsidian, Notepad-faithful copy modes, Live Preview, wikilinks and backlinks, tags, full-text search, the quick switcher, the calendar view, focus modes, image paste, and HTML and PDF export.
