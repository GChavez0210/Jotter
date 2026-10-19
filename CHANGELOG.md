# Changelog

All notable changes to Jotter are listed here, newest first. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

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
- **License.** Jotter is dual licensed under MIT or Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`).

### Changed

- Closing the window saves pending edits before the window closes.
- HTML export renders the note on the frontend before saving, so `export_active_note_html` takes finished HTML instead of Markdown.
- The Markdown export renderer (comrak) enables its `math_dollars` extension.

## [0.1.0]

First release: Markdown vaults compatible with Obsidian, Notepad-faithful copy modes, Live Preview, wikilinks and backlinks, tags, full-text search, the quick switcher, the calendar view, focus modes, image paste, and HTML and PDF export.
