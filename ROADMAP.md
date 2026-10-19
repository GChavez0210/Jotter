# Roadmap

Phases run from least to most effort. Each phase ships on its own, and items within a phase are independent unless noted.

Platform work (Linux, Android, Apple) sits in the last phase. Until then, build every feature without new Windows-only code, so the ports stay cheap.

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

## Phase 2: Core features (one to three weeks each)

- [ ] **Folder-based cloud sync.** Support vaults inside Google Drive, Dropbox, OneDrive, or Box folders. Detect each provider's conflict copies and document the setup.
- [ ] **Math calculation.** Evaluate inline expressions (`12*4 =` → `48`) and calc blocks with variables.
- [ ] **Note properties editor.** Edit YAML frontmatter through a form in the note header.
- [ ] **Outliner basics.** Fold lists, move items with `Alt+↑/↓`, indent and outdent, and zoom into a bullet.
- [ ] **Multi-dictionary spell check.** Bundle Hunspell dictionaries and choose languages per vault or per note.
- [ ] **Bring-your-own AI model.** Off by default. Supports OpenAI-compatible endpoints, Anthropic, and local Ollama, with keys in the OS keychain. Starts with three commands on the selection or the whole note: summarize, rewrite, and translate. Translate takes a target language, keeps Markdown formatting, links, and code blocks intact, and either replaces the text or inserts the translation below it.
- [ ] **Multi-language UI.** Extract UI strings into an i18n library and ship the most common languages.
- [ ] **Dictation.** Record in the app and insert the text at the cursor. Transcription uses local Whisper, which works offline, or the configured provider's transcription endpoint. With an AI model configured, an optional cleanup pass removes filler words, adds punctuation, and formats lists and headings as Markdown. A setting keeps the recording in `attachments/` and embeds it in the note.
- [ ] **Home page.** A start page of note cards in place of the empty "Nothing open" screen. Sections show recently opened, created, and modified notes, notes by tag, most and least linked notes, and a daily "Rediscover" pick of notes untouched for months. Each section has its own card count, sort order, and folder and tag filters, and a note appears only in the first section that matches it. Sections that filter by property depend on the properties editor. Data comes from the existing SQLite index.
- [ ] **Tags-and-links view.** Optional navigation with no folder tree. Files stay where they are on disk.
- [ ] **Task due dates with `.ics` export.** Parse a due-date syntax on checklist items and write a calendar file other apps can subscribe to.

## Phase 3: Larger features (a month or more each)

- [ ] **Property queries and dashboards.** A query language over the SQLite index, with query blocks in notes that render as live tables, card galleries, and task boards. Query blocks reuse the Home page's section renderer, so any note can become a dashboard. Blocks are declarative and run no scripts. Depends on the properties editor and the Home page.
- [ ] **Ask your vault.** AI answers grounded in your notes through a semantic search index. Depends on bring-your-own AI.
- [ ] **Typst blocks.** Render Typst code blocks to SVG. Markdown stays the file format.
- [ ] **Calendar task sync.** Push due dates to a calendar service over CalDAV. Depends on `.ics` export.

## Phase 4: Major projects and platforms

- [ ] **User plugins.** A stable public API, sandboxing, and a compatibility commitment.
- [ ] **Handwriting.** Pen input stored as SVG and embedded in notes.
- [ ] **Handwriting OCR.** Make handwriting searchable. Depends on handwriting.
- [ ] **Linux desktop.** Gate the Windows-only code (Mica, Win32 save dialog, Windows filename rules) and ship AppImage and `.deb` builds.
- [ ] **Android.** Tauri 2 mobile build with a touch-first UI. Depends on a sync story.
- [ ] **Native cloud sync APIs.** Provider sign-in, change tracking, and conflict handling for mobile, where folder sync is not available.
- [ ] **macOS and iOS.** Platform builds, iCloud Drive vaults, and iOS sandboxed file access.
- [ ] **Apple Notes import.** Read the Apple Notes database on macOS and convert notes to Markdown. Depends on macOS. Two-way sync is out of scope.
