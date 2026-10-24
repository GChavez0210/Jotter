# Roadmap

Phases run from least to most effort. Each phase ships on its own, and items within a phase are independent unless noted.

Desktop portability foundations are in place. Linux and Android are in Phase 3; macOS and iOS release work is in Phase 4. Keep platform-specific code behind adapters.

## Already shipped

- Markdown files as the source of truth, local-first, no database inside the vault
- Wikilinks, backlinks, and tags (inline and YAML frontmatter)
- Live Preview with bold, italic, headings, inline code, task checkboxes, and images
- Full-text search (tantivy) and fuzzy quick switcher (`Ctrl+O`)
- Calendar view of notes by created or modified date
- Viewport-bounded rendering, so typing stays instant in long notes

## Phase 1: Quick wins (hours to a couple of days each)

- [x] **Recently deleted view.** List notes Jotter sent to the Recycle Bin and restore them in place. The `trash` crate already provides list and restore.
- [x] **Daily journal.** One shortcut opens or creates today's note in `Daily/` from a template.
- [x] **Global summon hotkey.** Bring Jotter to the front from anywhere with `tauri-plugin-global-shortcut`, and keep it resident in the tray.
- [x] **Mermaid diagrams.** Render ```` ```mermaid ```` blocks with [Pele](https://github.com/obsidianmd/pele) (MIT, no dependencies, synchronous SVG output, themed through CSS variables) in Live Preview and in HTML/PDF export.
- [x] **Math formatting.** Render `$…$` and `$$…$$` with KaTeX in Live Preview and in export.
- [x] **Basic spell check.** Turn on the WebView spell checker in the editor, using the system's installed languages.
- [x] **Custom CSS snippets.** Load user `.css` files on top of the theme.
- [x] **Typo-tolerant search.** Add fuzzy term matching to full-text search.
- [x] **License.** Pick and publish an open-source license. Jotter is licensed under MIT.
- [x] **Single instance.** Launching Jotter while it is already running brings the open window to the front instead of starting a second instance. Restore a minimized or tray-hidden window. Use `tauri-plugin-single-instance`.
- [x] **Close button behavior.** A setting chooses what the window's X button does: quit Jotter, or hide it to the tray. Separate tray residency from the summon hotkey, which today turns on both. The tray menu's Quit always exits after saving.
- [x] **Narrow windows.** Lower the minimum window width below 640 px. When the window gets too narrow to fit the sidebar and the text column, hide the sidebar. When the user then shows the sidebar, widen the window just enough to fit both.
- [x] **Pinned notes.** Pin notes to the top of the sidebar for one-click access. Store pins per vault in app data.
- [x] **Outline panel.** List the open note's headings in the right panel, and jump to one with a click. Highlight the heading at the cursor.
- [x] **Right sidebar calendar and recently deleted items.** Move the calendar and recently deleted items to the right sidebar.
- [x] **Find and replace in a note.** `Ctrl+F` finds text in the open note and `Ctrl+H` replaces it, with match case, whole word, and regular expression options. Use `@codemirror/search` and translate its labels.
- [ ] **Settings export and import.** Settings live in WebView local storage today, so a reinstall or a new computer starts from defaults. Export them to a file, import them on another machine, and move their storage to the app data folder.
- [ ] **Task reminders.** Show a system notification when a task comes due. Due dates are already indexed. Use `tauri-plugin-notification`, with a setting to turn reminders off.
- [ ] **Reduce the frontend entry bundle.** Investigate Vite’s large-chunk advisory and split optional features where useful. Verify startup and editor behavior after changes.

## Phase 2: Core features (one to three weeks each)

- [x] **Folder-based cloud sync.** Support vaults inside Google Drive, Dropbox, OneDrive, or Box folders. Detect each provider's conflict copies and document the setup.
- [x] **Math calculation.** Evaluate inline expressions (`12*4 =` → `48`) and calc blocks with variables.
- [x] **Note properties editor.** Edit YAML frontmatter through a form in the note header.
- [x] **Outliner basics.** Fold lists, move items with `Alt+↑/↓`, indent and outdent, and zoom into a bullet.
- [x] **Multi-dictionary spell check.** Bundle Hunspell dictionaries and choose languages per vault or per note.
- [x] **Bring-your-own AI model.** Free, with no Jotter account or subscription required. Off by default. Supports OpenAI-compatible endpoints, Anthropic, and local Ollama, with keys in the OS keychain. Cloud BYOK calls go directly to the chosen provider; the user pays that provider. Local models run on the user's hardware. Starts with three commands on the selection or the whole note: summarize, rewrite, and translate. Translate takes a target language, keeps Markdown formatting, links, and code blocks intact, and either replaces the text or inserts the translation below it. A writing-style box in Settings → AI holds the user's own instructions for a Polish rewrite.
- [x] **Multi-language UI.** Extract UI strings into an i18n library (Paraglide JS) and ship English and Spanish. More languages need only a translated `messages/<locale>.json`.
- [x] **Home page.** A start page of note cards in place of the empty "Nothing open" screen. Sections show recently opened, created, and modified notes, notes by tag, most and least linked notes, and a daily "Rediscover" pick of notes untouched for months. Each section has its own card count, sort order, and folder and tag filters, and a note appears only in the first section that matches it. Sections that filter by property depend on the properties editor. Data comes from the existing SQLite index.
- [x] **Tags-and-links view.** Optional navigation with no folder tree. Files stay where they are on disk.
- [x] **Task due dates with `.ics` export.** Parse a due-date syntax on checklist items and write a calendar file other apps can subscribe to.
- [ ] **Callouts and footnotes.** Render Obsidian callouts (`> [!note]`, foldable with `+`/`-`) and Markdown footnotes in Live Preview and in export.
- [ ] **Update links on rename.** Renaming or moving a note offers to rewrite every `[[link]]` and Markdown link that points to it, using the index to find them. Show the number of affected notes first, and apply the change as one undoable batch per note.
- [ ] **Automatic updates.** Check for new releases with `tauri-plugin-updater`, show what changed, and install on restart. Requires update signing keys and a release feed, such as GitHub Releases.
- [ ] **Templates.** A template picker for new notes, fed by a templates folder. Variables cover date, time, title, and prompted values. This grows the daily-note template into a general feature.
- [ ] **Scheduled vault backups.** Zip the vault to a folder the user picks, on a schedule, keeping a set number of copies. Show the last backup time in Settings.
- [ ] **Vault health report.** List broken links, attachments no note uses, empty notes, and duplicate titles, with actions to fix or remove each.
- [ ] **Quick capture.** A small window on a global hotkey appends text to an inbox note without opening the main window. Shares the single-instance and tray work in Phase 1.
- [ ] **Split view.** Show two notes side by side, or a note next to its rendered preview, with independent scrolling.
- [ ] **Note version history.** Keep snapshots of each note in app data on save, thinned over time. A timeline with a diff view restores any earlier version. Protects against bad AI edits, sync mishaps, and accidental overwrites.
- [ ] **Accessibility audit.** Check the app against WCAG 2.2 with screen readers (NVDA, VoiceOver) and keyboard-only use, then fix focus order, labels, and contrast.
- [ ] **Calendar task sync.** Push due dates to a calendar service over CalDAV. Builds on the `.ics` export.

## Phase 3: Larger features (a month or more each)

- [ ] **Linux desktop.** Ship AppImage and `.deb` builds. Add a Linux CI job, use the native title bar, verify the clipboard on GNOME Wayland, KDE Wayland, and X11, make the summon hotkey X11-only, and default the close button to quit where trays are hidden. About one to two weeks.
- [ ] **Dictation.** Record in the app and insert the text at the cursor. Transcription uses local Whisper, which works offline, or the configured provider's transcription endpoint. With an AI model configured, an optional cleanup pass removes filler words, adds punctuation, and formats lists and headings as Markdown. A setting keeps the recording in `attachments/` and embeds it in the note.
- [ ] **Property queries and dashboards.** A query language over the SQLite index, with query blocks in notes that render as live tables, card galleries, and task boards. Query blocks reuse the Home page's section renderer, so any note can become a dashboard. Blocks are declarative and run no scripts. Builds on the properties editor and the Home page, both shipped.
- [ ] **Ask your vault.** AI answers grounded in your notes through a semantic search index. Builds on bring-your-own AI, already shipped.
- [ ] **Typst blocks.** Render Typst code blocks to SVG. Markdown stays the file format.
- [ ] **Word and styled PDF export.** Export notes to `.docx`, and add page headers, footers, page numbers, and a cover page to PDF export.
- [ ] **Web clipper.** A browser extension that saves the current page or a selection as Markdown in the vault, with its images.
- [ ] **Importers.** Convert Evernote (`.enex`), OneNote, Notion exports, Word documents, and folders of HTML into Markdown notes, keeping attachments, tags, and created dates.
- [ ] **Encryption and app lock.** Encrypt single notes or a whole vault with a password, and lock the app after a period of inactivity. Encrypted notes stay out of the search index unless unlocked.
- [ ] **IT deployment.** An MSI installer, a machine-wide policy file that can turn off AI or fix its provider, and proxy support for corporate networks.
- [ ] **P2P sync.** Sync a vault directly between your devices with end-to-end encryption and no account or cloud storage. Pair devices with a QR code; a relay forwards encrypted data only when a direct connection fails. Edits made on both devices become conflict copies. Android depends on it.
- [ ] **Android.** Tauri 2 mobile build with a touch-first UI: a sidebar drawer, a single-note view, and a formatting bar above the keyboard. Start with a vault in app storage; picking any folder through Android's storage framework comes later. Replace the clipboard, key storage, trash, and printing with Android versions. Depends on P2P sync. About two to three months.

## Phase 4: Major projects and platforms

- [ ] **User plugins.** A stable public API, sandboxing, and a compatibility commitment.
- [ ] **Handwriting.** Pen input stored as SVG and embedded in notes.
- [ ] **Handwriting OCR.** Make handwriting searchable. Depends on handwriting.
- [ ] **Native cloud sync APIs.** Provider sign-in, change tracking, and conflict handling for mobile, where folder sync is not available.
- [ ] **macOS release.** Signing, notarization, and Intel/Apple Silicon packaging. CI is configured for native build and unit-test checks. Release checks:
  - [ ] **Run native macOS CI.** Confirm the configured Mac build and Rust tests pass. Exercise Unix symlink containment, distinct case-sensitive vault index keys, and rename protection on a case-sensitive filesystem.
  - [ ] **Verify Mac window behavior.** Test native traffic lights, dragging, resizing, full screen, and reopening from the menu and Dock.
  - [ ] **Verify native Quit saves.** Confirm Cmd+Q and the app menu wait for dirty and in-flight saves, retain edits on save failure, and exit only after successful saving.
  - [ ] **Verify Mac dialogs.** Test vault selection and creation, HTML export, and task-calendar save dialogs, including cancellation and overwrite confirmation.
  - [ ] **Verify WKWebView editing and output.** Test copy, cut, clipboard image paste, bundled and System spelling, Cmd shortcuts, Option-based folding, printing, and PDF output.
  - [ ] **Verify Mac background operation.** Test summon shortcut registration, replacement, conflicts, disable behavior, and menu-bar residency.
  - [ ] **Verify Mac persistence and iCloud Drive.** Restart with recent vaults, personal dictionaries, and CSS snippets; confirm they persist outside the vault. Exercise downloaded iCloud Drive vault files and external changes.
  - [ ] **Complete Mac distribution checks.** Build and install signed, notarized app and DMG packages on supported Intel and Apple Silicon systems. Complete the manual macOS release checks.
  - [ ] **Evaluate in-app Mac Trash restoration.** The trash library cannot list or restore macOS Trash. Keep Finder restoration guidance until a supported implementation can restore notes without overwriting existing files.
- [ ] **iOS.** Mobile UI and sandboxed file access.
- [ ] **Apple Notes import.** Read the Apple Notes database on macOS and convert notes to Markdown. Depends on macOS. Two-way sync is out of scope.
- [ ] **Optional hosted AI subscriptions (undecided).** Revisit after real users have tried Jotter, as a possible revenue source. The draft plan: Basic at USD $5/month and Pro at USD $10/month, with the same AI features and different monthly allowances. BYOK stays free, and notes remain usable without an account. Builds on bring-your-own AI; a dictation allowance can follow once dictation ships.
