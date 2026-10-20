# Jotter

A Markdown note-taking desktop application compatible with Obsidian vaults, built with **Tauri 2**, **Rust**, **TypeScript**, **Svelte 5 (Vite)**, and **CodeMirror 6**.

Jotter's defining feature is **clipboard line break preservation**: copying text out of Jotter preserves single and multiple consecutive line breaks and indentation as written in the editor when pasted into **Google Chat, Slack, Gmail, Microsoft Word, and a plain-text editor**.

---

## The Three Copy Modes

Most Markdown editors and webview-based apps either collapse single `\n` line breaks into spaces (CommonMark soft-break rule), wrap paragraphs in `<p>` tags (which adds unwanted 8pt–12pt spacing in Word and Gmail), collapse leading indentation spaces in HTML targets, or copy dark-mode background colors onto the clipboard.

Jotter solves this by routing every copy (`Ctrl+C`) and cut (`Ctrl+X`) operation through Rust (`arboard`) with three selectable modes (accessible from the top command bar or **Settings → Copy**):

| Copy Mode | Clipboard Formats Written | Best For | How It Works |
| :--- | :--- | :--- | :--- |
| **1. Markdown source** *(Default)* | `text/plain` + `text/html` | Pasting raw Markdown notes, code snippets, or lists into **Google Chat, Slack, Gmail, Word, or Notepad** with exact line breaks and indentation | Writes raw Markdown to `text/plain` with normalized `\n` line breaks, and writes an HTML-escaped `text/html` representation where every `\n` becomes `<br>`, leading and repeated spaces become `&nbsp;`, and every tab (`\t`) becomes four `&nbsp;`. |
| **2. Plain** *(Opt-in "Clean Text")* | `text/plain` **only** *(no HTML)* | Pasting stripped, human-readable prose into plain-text fields, forms, or terminals without `**`, `#`, or `[[wiki]]` syntax | Uses `arboard::Clipboard::set_text` exclusively. Strips Markdown markers (`#`, `**`, `*`, `~~`, `` ` ``, `[label](url)` -> `label`, `[[Note\|Alias]]` -> `Alias` outside inline code spans) and drops code fence lines (` ``` ` / `~~~`) so the line count matches the code block content. |
| **3. Rich** | `text/html` + `text/plain` | Pasting formatted text (bold, italic, inline code, links) into **Gmail, Word, Google Docs, or Outlook** without extra paragraph spacing | Renders inline Markdown per line via `comrak`, drops code fence delimiter lines (` ``` ` / `~~~`), converts leading/repeated spaces and tabs to `&nbsp;`, and joins lines strictly with `<br>` instead of `<p>` blocks. Also provides clean plain text as the fallback stream. |

### Paste as Plain Text (`Ctrl+Shift+V`)
Pressing `Ctrl+Shift+V` (or clicking **Paste Plain** in the toolbar) reads `text/plain` directly from the system clipboard via Rust (`arboard::Clipboard::get_text`), normalizes `\r\n` / `\r` to `\n`, and inserts clean unstyled text at the cursor.

### Clipboard Image Paste (`Ctrl+V`)
When a vault is open, pasting an image from the system clipboard (such as a screenshot) automatically encodes and saves the PNG to `<vault>/attachments/Pasted image YYYYMMDD-HHMMSS.png` and inserts `![[Pasted image YYYYMMDD-HHMMSS.png]]` (or standard Markdown `![](attachments/...)`, configurable in **Settings → Editor**). Pasted images render inline in **Live** mode.

---

## Writing and Organizing

### Moving Notes
In the sidebar's **Folders** view, drag a note onto a folder to move it there. Drop it on the vault name above the tree to move it back to the root. Jotter saves pending edits first, keeps open tabs on the moved note, and refuses to replace another file with the same name.

### AI Menu
The AI menu opens in the center of the app window. Drag its **AI** header to reposition it. You can also focus the header and use the arrow keys; hold Shift for larger steps, or press Home to center it again.

The **Rewrite**, **Translate**, and **Summarize** tabs each keep their settings and preview while the menu is open. Switching tabs lets pending requests finish in the background. **Cancel** stops the current tab's request; closing the menu cancels all pending requests and clears its previews.

### Math
Write inline math as `$e^{i\pi} + 1 = 0$` and display math between `$$` lines. Live mode renders it with KaTeX; move the cursor into a block to edit the TeX, with a rendered preview below it. A `$` followed by a space, or a closing `$` followed by a digit, stays plain text, so `$5 and $10` is not treated as math.

### Diagrams
Fenced code blocks tagged `mermaid` render as diagrams through [Pele](https://github.com/obsidianmd/pele), a Mermaid renderer that follows Jotter's theme colors. A syntax error shows the message and line number in place of the diagram.

Math and diagrams also appear in HTML export and in Print to PDF. Exported HTML files use MathML, so they need no extra fonts or stylesheets.

### Calculations
End a line with an arithmetic expression and `=`, such as `Rent split: 1850 / 3 =`, and Jotter shows the result after the `=`. Press `Tab` or click the result to insert it. Expressions support `+ - * / ^`, parentheses, percentages (`200 - 25%`, `15% of 80`), `pi`, `e`, and the functions `sqrt`, `abs`, `round`, `floor`, `ceil`, `min`, `max`, `sin`, `cos`, `tan`, `log`, `ln`, and `exp`. A line needs an operator or function, so `x = 5` stays text.

For inline subtraction between integers, put spaces on both sides of the minus: `100 - 250 =`. Unspaced forms such as `100-250 =` stay text to avoid treating dates and phone numbers as calculations. Inside `calc` blocks, `100-250` still works.

A fenced code block tagged `calc` shows a result after each line:

````markdown
```calc
price = 12.5
qty = 4
price * qty        # 50
shipping = 7
sum                # 73.5, every result since the last blank line
```
````

`name = expression` assigns a variable for later lines. `sum` and `total` add up the results since the last blank line, and `prev` holds the previous result. `#` and `//` start comments. Results are not saved in the file, except in HTML export and Print to PDF, which write each result after its line.

### Properties
The note header shows the note's YAML frontmatter as a form. Each property has a type: text, list, number, checkbox, date, or date and time. Click a property's icon to change its type. **Add property** appears when you hover over the line under the title. Edits change only that property's lines in the file, so comments and formatting elsewhere in the frontmatter stay as they are, and `Ctrl+Z` undoes them. Values the form cannot edit, such as nested maps, show as read-only text; edit them in Source mode. Live mode hides the raw YAML; Source mode shows it.

### Outlining
Markdown lists work as an outline:

- `Alt+↑` and `Alt+↓` move the item at the cursor, with its children, past its neighbor. Outside a list, they move the line.
- `Tab` nests an item under the item above it, and `Shift+Tab` moves it back out.
- Numbered lists renumber after a move, indent, or outdent. A list keeps its first number, and an item that starts a new sublist becomes `1.`.
- These commands skip `- ` lines in code blocks and in YAML frontmatter.
- Click the arrow in the left margin to fold an item's children, or focus the arrow and press Enter or Space. `Ctrl+Shift+[` folds and `Ctrl+Shift+]` unfolds at the cursor, including heading sections.
- `Ctrl+.` shows only the item at the cursor and its children. Press `Ctrl+.` again on that item, or click **Show whole note**, to zoom out.

### Task Due Dates
Due dates inside backtick code spans stay literal and do not create calendar events or due-date chips.

Give a checklist item a due date in any of three common formats, each with an optional time:

```markdown
- [ ] Pay rent 📅 2026-11-01
- [ ] Call Sam due:2026-10-21 09:30
- [ ] Renew passport [due:: 2026-12-01]
```

Live mode shows the date as a chip, such as **Tomorrow** or **Nov 1**, which turns red once the date has passed. Move the cursor onto the line to edit the date. **Add Due Date to Task** in the command palette adds today's date to the task at the cursor.

To see due dates in a calendar app, choose a file in **Settings → Vault → Task calendar**. Jotter writes each open task as an event in that `.ics` file and rewrites it a few seconds after you edit, so a calendar subscribed to the file stays current. Tasks without a time are all-day events. Completed tasks are left out. Pick a file outside the vault, or inside a synced folder if your calendar app needs to reach it from another device. The file picker uses the native desktop dialog.

### Tags and Links
The Notes sidebar can list notes by tag instead of by folder. Click **Tags & links** above the list. Nested tags such as `#project/alpha` appear under their parent tag, and notes without tags appear under **No tags**. Expand a note to see the notes it links to. Files stay in their folders; switch back with **Folders**.

### Home Page
With a vault open and no note open, Jotter shows a Home page of note cards: recently opened, recently edited, new, most linked notes, and a daily **Rediscover** pick from notes untouched for three months. A note appears only in the first section that matches it. Click the gear to customize: rename, reorder, hide, or remove sections, set each one's card count and sort order, limit it to a folder or tag, or add a section for a tag. Each vault keeps its own layout. Turn the Home page off in **Settings → Vault**.

### Syncing Through a Cloud Folder
To use a vault on several computers, keep the vault folder inside a folder your sync client manages: Dropbox, OneDrive, Google Drive, or Box. Jotter reads and writes plain files, and it reloads notes that change on disk.

1. Move or create the vault inside the synced folder, such as `C:\Users\you\Dropbox\Notes`.
2. Keep the vault available offline. In OneDrive, right-click the vault folder and choose **Always keep on this device**. In Google Drive for desktop, choose **Available offline**. In Dropbox, choose **Make available offline**. Online-only files make search and backlinks wait for downloads.
3. Open the vault in Jotter on each computer. Indexes and edit history stay in the local OS application data folder on each computer, so the sync client never copies them.
4. Let the sync client finish before you edit the same note on another computer.

When two computers change a note before syncing, the sync client keeps both versions and names one as a conflict copy, such as `Plan (Alex's conflicted copy 2026-10-19).md` (Dropbox), `Plan-DESKTOP-4F2K9.md` (OneDrive), `Plan (1).md` (Google Drive and Box), or `Plan.sync-conflict-20261019-101500-ABCDEFG.md` (Syncthing). Jotter lists these copies in the **Sync conflicts** sidebar view and in the status bar. Open the copy and the original, move the changes you want into the original, and move the copy to the Recycle Bin. **Settings → Vault → Cloud sync** shows which client Jotter detected. Jotter guesses the client from the vault path, and it matches the OneDrive, Google Drive, Box, and iCloud names only in vaults inside those folders, so a note named `Plan (1).md` in a local vault is not flagged. Those names are ambiguous even inside a sync folder, since `Lecture 2.md` can be a real note. Select **Mark as not a conflict** (the check mark) to stop listing it.

### Daily Notes
`Ctrl+Shift+D` opens today's note, `Daily/YYYY-MM-DD.md`. If the note does not exist, Jotter creates it from the template note `Templates/Daily.md`, replacing `{{date}}`, `{{title}}` and `{{time}}`. Without a template note, Jotter uses a built-in one. Set the folder and template in **Settings → Vault**.

### Recently Deleted
Notes and folders you delete go to the Recycle Bin. The **Recently deleted** sidebar view lists the ones from the open vault, newest first, and restores them to their original location. Jotter does not overwrite a file that now exists at that path.

### Typo-Tolerant Search
Full-text search (`Ctrl+Shift+F`) also finds words one or two letters off, so `recieve` finds `receive`. Exact matches rank first. Words of three letters or fewer, quoted phrases, and queries with operators such as `AND` or `title:` match exactly.

### Spell Check
The editor underlines misspelled words. Right-click a word for suggestions, or add it to your personal dictionary. Turn spell check off in **Settings → Editor**.

Jotter ships Hunspell dictionaries for English (United States and United Kingdom), Spanish, French, German, Portuguese (Brazil), and Italian. **Settings → Editor → Spelling dictionaries** picks the languages for the open vault; a word counts as correct when any of them accepts it. A note can use other languages through its `lang` property:

```yaml
---
lang: [en, es]
---
```

Tags such as `es-MX` match the closest dictionary, here `es`. A note in a language with no installed dictionary gets no underlines. Set `spellcheck: false` to skip one note.

To add a language, put a Hunspell `.aff` and `.dic` pair with the same name, such as `nl.aff` and `nl.dic`, in the app data dictionaries folder (**Settings → Editor → Open folder**), then select **Reload**. Words you add from the editor go to `personal.txt` in the same folder. Choose **System** instead of **Jotter** to use the WebView's checker and the native checker for every note (language coverage depends on the OS and WebView).

### Summon Hotkey
Turn on **Settings → System → Summon hotkey** to show or hide Jotter from any app with `Ctrl+Alt+J`, or a hotkey you choose. While it is on, Jotter keeps an icon in the tray, and closing the window hides it instead of quitting. Quit from the tray icon's menu; Jotter saves open notes before it exits.

### CSS Snippets
Put `.css` files in the app data `snippets` folder, then turn each one on in **Settings → System → CSS snippets**. Snippets load after the theme, so they can override its colors, fonts and spacing. **Open folder** opens the snippets folder, and **Reload** picks up edits.

---

## Keyboard Shortcuts

Shortcut labels use Ctrl on Windows/Linux and Cmd on macOS. Folding uses Cmd+Option+[ and Cmd+Option+] on macOS.

### File, Vault & Navigation
| Shortcut | Action |
| :--- | :--- |
| `Ctrl+N` | Create new Markdown note in vault |
| `Ctrl+O` | Open Quick Switcher (fuzzy search by note title & path) |
| `Ctrl+P` | Open Command Palette (available in all modes, including Zen mode) |
| `Ctrl+Shift+F` | Open full-text Vault Search (`tantivy` with highlighted snippets, tolerates typos) |
| `Ctrl+Shift+D` | Open today's daily note, creating it from the template if needed |
| `Ctrl+S` | Flush autosave for the active note immediately |
| `Ctrl+W` | Close active note tab |
| `Ctrl+\` | Toggle left sidebar (or exit Focus Mode) |
| `Ctrl+,` | Open Settings dialog (`Editor`, `Copy`, `Appearance`, `Vault`, `System`) |
| `Ctrl+Alt+J` | Summon hotkey: show or hide Jotter from any app (off by default; set in Settings > System) |
| `Ctrl+Shift+P` | Print / Export active note to PDF (`Microsoft Print to PDF`) |
| `Esc` | Close active modal/dialog or exit Focus Mode (`Minimal` / `Zen`) |

### Editing Modes & Focus
| Shortcut | Action |
| :--- | :--- |
| `Ctrl+/` | Toggle between **Live Preview** mode and **Source** mode |
| `Ctrl+Shift+M` | Toggle **Minimal Focus Mode** (hides sidebar, tabs, toolbars, and status bar) |
| `F11` | Toggle **Zen Fullscreen Mode** (fullscreen window with only the centered editor canvas) |

### Markdown Formatting & Clipboard
| Shortcut | Action |
| :--- | :--- |
| `Ctrl+C` / `Ctrl+X` | Copy / Cut selection (or current line if nothing is selected) using active Copy Mode |
| `Ctrl+Shift+V` | Paste as plain text (`text/plain` normalized to `\n`) |
| `Ctrl+V` | Paste text or save clipboard image to `attachments/Pasted image YYYYMMDD-HHMMSS.png` |
| `Ctrl+B` | Toggle **Bold** (`**text**`) |
| `Ctrl+I` | Toggle *Italic* (`*text*`) |
| `Ctrl+Shift+S` | Toggle ~~Strikethrough~~ (`~~text~~`) |
| `Ctrl+E` | Toggle `Inline Code` (`` `code` ``) |
| `Ctrl+K` | Insert Markdown link (`[text](url)`) |
| `Ctrl+1` … `Ctrl+6` | Set Heading level 1–6 (`#` … `######`) |
| `Ctrl+Shift+8` | Toggle bullet list (`- `) |
| `Ctrl+Shift+9` | Toggle task list checkbox (`- [ ] `) |
| `Ctrl+Shift+.` | Toggle blockquote (`> `) |
| `Tab` | Insert a pending calculation result; in a list, nest the item under the one above |
| `Shift+Tab` | Move a list item out one level |
| `Alt+↑` / `Alt+↓` | Move the list item, with its children, or the line |
| `Ctrl+.` | Zoom into the list item at the cursor; again on that item to zoom out |
| `Ctrl+Shift+[` / `Ctrl+Shift+]` | Fold / unfold the list item or heading section at the cursor |

To fold or unfold everything, use **Fold All** and **Unfold All** in the command palette. They have no shortcut, because Windows reports AltGr as `Ctrl+Alt`, and many keyboard layouts type `[` and `]` with AltGr.

### File Tree & Calendar Keyboard Navigation
| Shortcut | Context | Action |
| :--- | :--- | :--- |
| `Enter` | File Tree item | Open note or expand/collapse folder |
| `F2` | File Tree item | Rename selected note or folder inline |
| `Delete` | File Tree item | Move selected note or folder to the system trash (with confirmation) |
| `Arrow Keys` | Calendar Month Grid | Move day selection (`Left`/`Right` ±1 day, `Up`/`Down` ±7 days across month boundaries) |
| `T` | Calendar Month Grid | Jump to Today and select today's notes |

---

## Desktop prerequisites

- **Rust** (stable toolchain, version 1.88 or later, via `rustup`)
- **Node.js** (v20+ and `npm`)
- **Windows:** Microsoft C++ build tools and the Edge WebView2 Runtime.
- **macOS preparation:** Xcode command-line tools and the system WKWebView. A native Mac build and release smoke test are still required.

## Build & Run Instructions

Install frontend dependencies:

```sh
npm install
```

Run in development mode:

```sh
npm run tauri dev
```

Build native desktop bundles under `src-tauri/target/release/bundle/` (Windows installers on Windows; app and DMG on macOS):

```sh
npm run tauri build
```

Application data lives outside vaults: `%LOCALAPPDATA%\Jotter` on Windows and `~/Library/Application Support/Jotter` on macOS. The macOS config keeps native traffic lights and uses an opaque window. Shared-vault filename checks retain conservative rules so files can move between systems. On macOS, restore deleted notes through Finder Trash.

## Verification & Quality Checks

Run Rust unit tests and Clippy lints:

```sh
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Run frontend Svelte + TypeScript checks (`svelte-check` and `tsc --noEmit`) and production bundle build:

```sh
npm run check
npm test
npm run build
```

After building on Windows, run the isolated desktop checks with `node scripts/verify-desktop.mjs`. The script uses the release executable, a disposable vault, separate app data, and a separate WebView2 profile under `src-tauri/target/desktop-verification/`. It exercises real rendering, Recycle Bin restore, save failures, tray residency, and delayed saves. Screenshots and results stay in that directory. It does not install the app.

See [MANUAL_TESTS.md](MANUAL_TESTS.md) for manual paste verification across Google Chat, Slack, Gmail, Word, and Notepad, and [DECISIONS.md](DECISIONS.md) for architectural and accessibility documentation. See [ROADMAP.md](ROADMAP.md) for planned features.

## License

Jotter is licensed under the [MIT License](LICENSE-MIT).

Contributions submitted for inclusion in Jotter are licensed under the MIT License.

The bundled spelling dictionaries are separate works under their own licenses, installed with each dictionary as `dictionaries/<language>/LICENSE`. They come from [wooorm/dictionaries](https://github.com/wooorm/dictionaries): English from SCOWL (MIT and BSD), Spanish (GPL-3.0, LGPL-3.0, or MPL-1.1), French (MPL-2.0), German (GPL-2.0 or GPL-3.0), Portuguese (LGPL-3.0 or MPL-2.0), and Italian (GPL-3.0).
