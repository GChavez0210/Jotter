# Manual Clipboard Verification Tests (Milestone 1)

Run Jotter with `npm run tauri dev` (or `cargo tauri dev` if `cargo-tauri` is installed). The editor opens with a pre-loaded test note containing single line breaks, multiple consecutive blank lines, inline formatting, task lists, and Obsidian `[[wiki links]]`.

## Sample Test Block to Select & Copy

Select lines 6–18 in the editor (or copy the whole note with `Ctrl+C`):

```markdown
## 1. Consecutive Single Line Breaks
Line A: First line with **bold text** and *italic text*.
Line B: Immediately below Line A (single newline, never collapsed into a space).
Line C: Immediately below Line B with `inline code` and ~~strikethrough~~.

## 2. Preserved Blank Lines (Single and Double Gaps)
Paragraph 1: Separated from Paragraph 2 by one blank line.

Paragraph 2: Separated from Paragraph 3 by two blank lines.


Paragraph 3: Exact blank line count stays intact just like Windows Notepad.
```

---

## Target Application Paste Matrix

### 1. Google Chat (Web / Desktop App)

| Copy Mode | Action | Expected Result |
| :--- | :--- | :--- |
| **Plain** (default) | Copy sample block (`Ctrl+C`), paste (`Ctrl+V`) into chat compose box | Markdown syntax (`##`, `**`, `*`, `` ` ``, `~~`) is stripped. `Line A`, `Line B`, and `Line C` appear on separate consecutive lines (not merged onto one line). Single and double blank lines between `Paragraph 1–3` match the editor line-for-line. |
| **Rich** | Switch toolbar to **Rich**, copy (`Ctrl+C`), paste (`Ctrl+V`) | `bold text`, *italic text*, `inline code`, and ~~strikethrough~~ render with formatting where supported. Every single newline is preserved via `<br>` without extra paragraph gaps. |
| **Markdown source** | Switch toolbar to **Markdown source**, copy (`Ctrl+C`), paste (`Ctrl+V`) | Raw Markdown markers (`##`, `**bold text**`, `[[Product Spec|Jotter Product Spec]]`) are pasted verbatim with all single and blank lines preserved. |

### 2. Slack (Desktop App / Web)

| Copy Mode | Action | Expected Result |
| :--- | :--- | :--- |
| **Plain** (default) | Copy sample block (`Ctrl+C`), paste (`Ctrl+V`) into message composer | Clean unformatted text is pasted with exact `\n` line breaks. Consecutive lines stay on separate lines; blank lines are preserved 1-to-1. |
| **Rich** | Switch to **Rich**, copy (`Ctrl+C`), paste (`Ctrl+V`) | Rich formatting (**bold**, *italic*, `code`, ~~strikethrough~~) transfers into Slack's WYSIWYG composer while preserving single line breaks (`<br>`) and blank lines. |
| **Markdown source** | Switch to **Markdown source**, copy (`Ctrl+C`), paste (`Ctrl+V`) | Literal Markdown syntax (`**`, `#`, `[[...]]`) is pasted with exact line breaks. |

### 3. Gmail (Web Compose Window)

| Copy Mode | Action | Expected Result |
| :--- | :--- | :--- |
| **Plain** (default) | Copy sample block (`Ctrl+C`), paste (`Ctrl+V`) into email body | Text pastes in Gmail's default font with zero background/color pollution. Every `\n` is converted to `<br>`, so `Line A`, `Line B`, and `Line C` sit directly under one another without `<p>` double-spacing, and blank lines between `Paragraph 1–3` are preserved 1-to-1. |
| **Rich** | Switch to **Rich**, copy (`Ctrl+C`), paste (`Ctrl+V`) | Headings, **bold**, *italic*, `inline code`, and ~~strikethrough~~ appear formatted in the email body. Line breaks use `<br>` so consecutive lines do not have unwanted paragraph margins. |
| **Markdown source** | Switch to **Markdown source**, copy (`Ctrl+C`), paste (`Ctrl+V`) | Raw Markdown characters (`#`, `**`, `[[wiki]]`) paste cleanly with `<br>` line breaks. |

### 4. Microsoft Word (Desktop on Windows)

| Copy Mode | Action | Expected Result |
| :--- | :--- | :--- |
| **Plain** (default) | Copy sample block (`Ctrl+C`), paste (`Ctrl+V`) into a Word document | Because `text/html` uses `<br>` instead of `<p>`, Word inserts manual line breaks (`Shift+Enter`) rather than adding Word's default 8pt paragraph spacing between `Line A`, `Line B`, and `Line C`. Line breaks look identical to Windows Notepad. |
| **Rich** | Switch to **Rich**, copy (`Ctrl+C`), paste (`Ctrl+V`) | **Bold**, *italic*, `code`, and ~~strikethrough~~ transfer into Word while preserving exact `<br>` line spacing without extra paragraph gaps. |
| **Markdown source** | Switch to **Markdown source**, copy (`Ctrl+C`), paste (`Ctrl+V`) | Literal Markdown syntax pastes with exact single-spaced line breaks. |

### 5. Windows Notepad

| Copy Mode | Action | Expected Result |
| :--- | :--- | :--- |
| **Plain** (default) | Copy sample block (`Ctrl+C`), paste (`Ctrl+V`) into Notepad | Notepad reads the `text/plain` alternative (`Some(plain)`) written by `arboard::Clipboard::set_html`. Markdown markers are stripped and every `\n` line break matches the selection 1-to-1. |
| **Rich** | Switch to **Rich**, copy (`Ctrl+C`), paste (`Ctrl+V`) | Notepad reads the `text/plain` alternative and pastes clean stripped plain text with exact line breaks. |
| **Markdown source** | Switch to **Markdown source**, copy (`Ctrl+C`), paste (`Ctrl+V`) | Raw Markdown text pastes into Notepad with exact line breaks. |

---

## Paste-as-Plain-Text Test (`Ctrl+Shift+V`)

1. Copy styled rich text from a web page, Word document, or Gmail (containing fonts, colors, tables, or headings).
2. Click inside the Jotter editor and press **`Ctrl+Shift+V`** (or click **Paste Plain** in the toolbar).
3. **Expected result**: Only clean plain text with normalized `\n` line breaks is inserted into the CodeMirror document; all HTML tags and rich styles are ignored.
