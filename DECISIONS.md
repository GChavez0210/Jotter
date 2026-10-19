# Architectural & Design Decisions (`DECISIONS.md`)

## 1. Dual-Format Clipboard Architecture (`arboard::Clipboard::set_html`)
- **Problem**: Standard Markdown editors and browser-based webviews either collapse single `\n` line breaks into spaces (CommonMark soft-break rule), wrap paragraphs in `<p>` tags (which adds 8pt–12pt paragraph margins in Microsoft Word and Gmail), or attach inline CSS font/background styles from the code editor DOM.
- **Decision**: Every copy operation in Jotter routes through Rust (`copy_to_clipboard`) and calls `arboard::Clipboard::set_html(html, Some(plain))`, writing two clipboard formats at once:
  1. `text/plain` (`CF_UNICODETEXT` on Windows): normalized `\n` line breaks.
  2. `text/html` (`CF_HTML` on Windows): line-by-line HTML joined strictly by `<br>` (never `<p>`), so target apps that prefer `text/html` (Gmail, Word, Google Chat, Slack) preserve exact 1-to-1 line breaks and blank lines (`\n\n` -> `<br><br>`) just like Windows Notepad.
- **Three Copy Modes**:
  - **Plain** (default): Strips Markdown syntax (`#`, `**`, `*`, `~~`, `` ` ``, `[label](url)`, `[[Target|Alias]]`) into clean plain text in `text/plain` (`\n`) and HTML-escaped plain text in `text/html` (`<br>`).
  - **Rich**: Renders inline/block Markdown via `comrak` per line and joins lines with `<br>` in `text/html`, paired with stripped plain text in `text/plain`.
  - **Markdown source**: Preserves raw Markdown syntax verbatim in `text/plain` (`\n`) and HTML-escaped raw Markdown in `text/html` (`<br>`).

## 2. Windows 11 Mica Backdrop & Native Frame / Snap Layouts
- **Decision**: Keep native Windows decorations enabled (`"decorations": true, "transparent": true` in `tauri.conf.json`) and apply Mica via `window_vibrancy::apply_mica(&window, dark_mode)`.
- **Rationale**: Keeping native window decorations guarantees 100% compatibility with Windows 11 Snap Layouts (hovering over the maximize button), native caption buttons, window edge resize handles, and system window animations while allowing the Mica backdrop to show through the transparent WebView2 surface.
- **Fallback**: When `apply_mica` returns an error (such as on Windows 10 or when OS transparency effects are off), or when the user enables the solid-color fallback in Settings, the UI switches to opaque Fluent surface colors (`#f3f3f3` in light mode, `#202020` in dark mode).

## 3. SQLite Index & File Modification Times Under Sync Tools
- **Note on Sync Tools**: File synchronization tools (OneDrive, Dropbox, Syncthing, Git, Obsidian Sync) can alter filesystem `mtime` (`modified` timestamps) when syncing files across machines, which can skew a calendar view that relies solely on filesystem `mtime`.
- **Decision**: The SQLite index is strictly a rebuildable cache derived from `.md` files in the vault. In addition to indexing filesystem `modified` and `created` timestamps (refreshed by `notify`), in-app save events can be logged in SQLite so Jotter can prioritize known in-app edit timestamps while falling back to filesystem `mtime` on cache rebuild.
