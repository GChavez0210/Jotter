# Jotter

A Windows-native Markdown note-taking desktop application compatible with Obsidian vaults, built with **Tauri 2**, **Rust**, **TypeScript**, **Svelte 5 (Vite)**, and **CodeMirror 6**.

Its defining feature is **Notepad-faithful clipboard line break preservation**: copying text writes both `text/plain` (with real `\n` line breaks) and `text/html` (where every newline becomes `<br>`) via `arboard`'s `set_html`, ensuring single and multiple line breaks paste cleanly into Google Chat, Slack, Gmail, Microsoft Word, and Windows Notepad.

## Prerequisites (Windows 10 / 11)

- **Rust** (stable toolchain via `rustup`)
- **Node.js** (v20+ and `npm`)
- **Microsoft Edge WebView2 Runtime** (pre-installed on Windows 11)

## Build & Run Instructions

Install frontend dependencies:

```powershell
npm install
```

Run in development mode:

```powershell
npm run tauri dev
# Or if cargo-tauri CLI is installed globally:
cargo tauri dev
```

Build a production Windows installer / bundle:

```powershell
npm run tauri build
# Or with cargo-tauri CLI:
cargo tauri build
```

## Verification & Quality Checks

Run Rust unit tests and Clippy lints (from `src-tauri/`):

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Run frontend TypeScript and production bundle checks:

```powershell
npm run check
npm run build
```

See [MANUAL_TESTS.md](MANUAL_TESTS.md) for manual paste verification across Google Chat, Slack, Gmail, Word, and Notepad, and [DECISIONS.md](DECISIONS.md) for architectural notes.
