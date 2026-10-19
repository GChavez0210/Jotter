# Codebase and Phase 1 review

Review date: October 19, 2026. Scope: application-owned Svelte/TypeScript and Rust code, Tauri configuration, dependency lockfiles, export pipeline, and all nine Phase 1 roadmap items. The findings below record the original 0.1.0 behavior and source locations. All eleven application findings have been addressed in 0.1.1; see the remediation results at the end. Licensing is MIT only.

## Findings

### 1. P1: Failed saves do not prevent closing or switching vaults

`src/lib/stores/tabs.svelte.ts:214` catches save failures and returns normally. `closeTab` then removes the dirty tab. The title-bar close and vault-switch flows also continue after `flushAllAutosaves`. The tray listener in `src/App.svelte:336` calls `quitApp` in a `finally` block.

Confirmed with the compiled Svelte store and a mocked failing disk write: closing a dirty tab logged `Autosave failed: disk write failed`, then left zero open tabs. Return a save result or propagate the failure; keep dirty tabs and the window open until saving succeeds or the user explicitly discards edits.

### 2. P1: Native close events bypass the close-save fix

`src-tauri/src/lib.rs:934` only prevents `CloseRequested` when tray residency is enabled. Alt+F4 and Windows' native Close action can therefore destroy the nonresident window without passing through `TitleBar.svelte`'s flush. Closing within the 380 ms autosave debounce can lose the latest edits. Route every native close event through one save/close handshake. This is source-confirmed; native Windows UI behavior was not exercised in this review.

### 3. P1: Tray Quit forces exit after five seconds

`src-tauri/src/tray.rs:51` starts an unconditional exit timer after emitting `quit-requested`. A slow disk, large indexing workload, or failed save can leave edits unsaved when that timer fires. An event-emission failure also exits immediately. Await successful save acknowledgement and provide a recoverable error state instead of forcing a timed exit.

### 4. P1, security: Junctions bypass the vault boundary

`src-tauri/src/vault.rs:88` validates path components but never checks where the resulting path resolves. `src-tauri/src/media_export.rs:229` has the same boundary gap for attachments. The index walker follows directory paths without cycle detection (`src-tauri/src/index.rs:467`).

Confirmed using a temporary vault whose `linked` directory was a Windows junction to a sibling directory: `read_vault_note(vault, "linked/Secret.md")` returned the sibling's contents. The same resolver serves saves and renames. A junction cycle can also cause unbounded walking. Enforce resolved-path containment for existing files and write parents, and reject or deliberately handle reparse points and directory cycles. Both probe directories stayed inside the workspace.

### 5. P1: Some valid Unicode note names panic

`src-tauri/src/vault.rs:133` slices a UTF-8 string at `len - 3` before appending `.md`. That index can fall inside a character. A probe calling `ensure_md_extension("日a")` reproduced the panic. Creating or renaming a note with that name can crash the command; an uncaught panic can terminate the application. Check the suffix through a boundary-safe operation instead of an arbitrary string byte slice.

### 6. P1: An asynchronous cut can delete text from another note

`src/lib/Editor.svelte:537` awaits clipboard IPC, then dispatches a deletion using offsets captured before the await. Switching tabs reuses the same `EditorView` with another state. If the clipboard reply arrives afterward, the deletion applies to the other note. Plain-text and image paste have related document-identity gaps. Capture the destination document identity and map positions through edits, or cancel the operation when its document changes. Source-confirmed; no native clipboard race was exercised.

### 7. P2: Image previews leak across vaults and retain old bytes

`src/lib/livePreview.ts:24` caches image data by target string alone. `ImageEmbedWidget` uses that cache at line 227. The exported `clearImagePreviewCache` function has no callers. Opening two vaults that both embed `attachments/photo.png` can display the first vault's image in the second. Replacing an image on disk also leaves the old preview cached. Key by vault identity and invalidate changed attachments; add a memory limit.

### 8. P2: Folder names containing SQL wildcards affect unrelated index rows

`src-tauri/src/index.rs:562` interpolates a folder name into a `LIKE` pattern without escaping `_` and `%`. A probe deleting the indexed prefix `A_B` also removed `AxB/b.md`. Disk files survive, but backlinks, tags, and calendar entries disappear until rebuilding. `rename_edit_log_paths` at line 286 shares this pattern and can rewrite unrelated history. Escape wildcard characters with an explicit `ESCAPE` clause or compare literal path prefixes.

### 9. P2: External edits retain an older in-app modification date

`src-tauri/src/index.rs:512` prefers any saved edit-log timestamp over the file's modification time. A probe with an in-app timestamp of 1000 and a later filesystem timestamp of 2000 stored 1000. After a user edits a note in another editor, the calendar can continue showing its earlier Jotter edit date. Define which timestamp the calendar represents and account for later external edits when representing modification time.

### 10. P2: Live Preview's rich-block parser disagrees with Markdown export

`src/lib/livePreviewBlocks.ts:28` scans raw lines instead of using Markdown block context. Probes confirmed that four-space-indented `$$ x $$` becomes a rendered math widget even though Markdown treats it as code, while a Mermaid fence inside a blockquote produces no rich block. Use Markdown syntax context to identify code, containers, and eligible math. The basic top-level Mermaid case succeeded.

### 11. P2: Printing diagrams from dark mode keeps light text on white paper

`src/App.svelte:108` extracts only the article and drops the export document's diagram stylesheet. `src/app.css:211` assigns diagram foreground, line, and surface variables from the active app theme. The print-specific override at line 225 changes only the background to white. Dark-theme diagram labels therefore retain their light foreground on white. Add a complete light diagram palette under `@media print`. Source-confirmed; actual PDF output was not visually inspected.

## Optimization opportunities

- `src/lib/livePreviewBlocks.ts:182` rescans every line on every document change. A Node probe measured about 21.8 ms per scan for 100,001 ordinary text lines on this machine, excluding CodeMirror layout and other editor work. Update affected blocks incrementally.
- `src/lib/Editor.svelte:477` converts the full document to a string and splits it for statistics on selection changes as well as edits. `NoteHeader.svelte` also scans the full note for tags on edits. Cache or debounce these computations; update selection statistics separately.
- `src-tauri/src/index.rs:538` re-resolves every link after each individual note upsert. Folder restore, rename, and watcher batches repeat that whole-vault operation. Batch index mutations in a transaction and resolve links once per batch; batch Tantivy commits too.
- `RecentlyDeletedPanel.svelte:46` refreshes on every index-version change, including autosaves. The synchronous backend command enumerates the OS Recycle Bin. Refresh on deletion/restoration or explicit request and move expensive enumeration away from the UI thread.
- `vite.config.ts:11` disables JavaScript and CSS minification for production. The build's main JavaScript chunk is about 1.90 MB before gzip. Re-enable production minification and retain readable development builds.
- `src-tauri/tauri.conf.json` disables CSP. Raw Markdown HTML is disabled and KaTeX uses `trust: false`, which are useful controls. Add a tested CSP as defense in depth, accounting for CodeMirror styles, fonts, images, and intentional user CSS snippets. This review did not demonstrate script injection.

## Phase 1 cross-check

| Roadmap item | Assessment |
| --- | --- |
| Recently deleted | Listing, vault filtering, restore, overwrite refusal, and reindexing are implemented. Unit tests cover filtering; real Recycle Bin restore remains unverified. Enumeration frequency needs optimization. |
| Daily journal | Shortcut, local-date filename, configurable folder/template, placeholder replacement, existing-note reuse, and parent-folder creation are implemented. Native end-to-end interaction remains unverified. |
| Global summon hotkey | Registration, pressed-event toggle, tray residency, and tray menu are implemented. Native registration conflicts and foreground behavior remain unverified. Quit has the data-loss issues above. |
| Mermaid diagrams | Shared renderer supports top-level Live Preview and export. Container parsing and dark-mode printing have open findings. |
| Math formatting | KaTeX is lazily loaded with untrusted commands disabled. Export marks math correctly in the Rust test. Live Preview misclassifies indented code; browser and PDF layout remain unverified. |
| Basic spell check | Editor content attributes and settings toggle are wired. Windows dictionary behavior, correction menu, and visible underlines remain unverified. |
| CSS snippets | App-data folder discovery, file-size cap, enabled-list persistence, reload, and style application are implemented. Rust discovery tests pass; visible restyling remains unverified. |
| Typo-tolerant search | Tests pass for `recieve`/`receive`, transpositions, exact-match ranking in the tested example, short-word exclusion, and explicit query syntax. No additional confirmed correctness issue found in this item. |
| License | Changed to MIT only in npm metadata, lockfile root metadata, Cargo metadata, README, roadmap, and changelog. Apache license file removed; MIT text retained. |

The separate close fix is incomplete. The Phase 1 checkboxes establish implementation presence, not full Windows UI validation.

## Verification and limits

- `npm run check`: passed with zero errors and zero warnings.
- `npm run build`: passed.
- Rust library tests: 40 passed, with the native clipboard roundtrip excluded. Tests used a workspace-local temporary directory because the sandbox's default temporary directory denied atomic renames.
- Native clipboard roundtrip: the initial run failed because the system clipboard was unavailable; it was not counted as a passing check or an application regression.
- Four temporary Rust probes reproduced the junction escape, Unicode panic, wildcard index deletion, and external timestamp issue. All four assertions passed because they asserted the observed buggy behavior; they are not regression tests for fixes.
- A compiled-store probe reproduced tab removal after a failed save. CodeMirror parser probes reproduced the rich-block cases above.
- `npm audit --json`: zero reported advisories.
- `cargo audit`: zero entries in its vulnerability category, but three unsoundness warnings and two maintenance warnings. `lru 0.12.5` is pulled in by `tantivy 0.22.1` on this host. RustSec flags its mutable iterator and panic-safety behavior (`RUSTSEC-2026-0002`, `RUSTSEC-2026-0253`). The lockfile also includes the platform-specific `glib 0.18.5` warning (`RUSTSEC-2024-0429`), which is absent from this host's active dependency tree. Upgrade the relevant dependency chains and verify compatibility; this review did not prove that Jotter reaches the affected methods. `instant` and `proc-macro-error` are flagged as unmaintained.
- No live Windows UI, native hotkey, dictionary, Recycle Bin restore, or PDF visual validation was performed. A source review and these probes do not establish that every remaining path is bug-free.

Existing edits to the sample daily note and the untracked `.claude` directory were preserved. No commit or push was made.

## Version 0.1.1 remediation results

All eleven application findings are implemented. Failed saves propagate a failure result; saves for one note share an in-flight operation and include edits made while waiting. All native close requests and tray Quit use a save acknowledgement, with no forced timeout. The app blocks further editing during that handshake and reopens after a save failure.

Vault paths reject symlinks and Windows reparse points below the chosen vault root, including write parents, attachment paths, restore targets, and indexing. Walkers skip links and junction cycles. A vault-operation lock also prevents an old watcher or asynchronous restore from updating a newly opened vault's shared indexes.

Unicode suffix checks are boundary-safe. Clipboard mutations retain their original document identity and selection, and cancel when that document changes. Image previews have bounded storage, vault/file-change invalidation, and stale-response rejection. SQLite folder patterns escape wildcards. Modification dates account for newer external edits while retaining edit history.

Rich-block detection uses Markdown block context, including quoted and list-contained Mermaid. Unchanged widgets map through edits; reparsing happens after typing pauses. Print uses a complete light diagram palette and extracts only the export's main content. Real-app testing caught and fixed an additional issue where the old article-only extraction inserted the export's document styles into the live app.

Index batches use a transaction and resolve links once; watcher batches commit Tantivy changes together. Selection-only statistics reuse document totals, and document statistics and tag scans defer while typing. Recycle Bin enumeration runs off the UI thread and no longer refreshes for autosaves. Production JavaScript and CSS are minified, and CSP is enabled. The main JavaScript chunk shrank from about 1.90 MB to 791 KB before gzip.

### Automated results

- `npm run check`: zero errors and warnings.
- `npm test`: six frontend regressions passed, including failed saves, edits during an in-flight save, clipboard identity, cache invalidation, rich-block context, and daily-template dates.
- Rust: 40 library tests and five filesystem/index regressions passed. The native clipboard roundtrip test was excluded because it changes the user's clipboard; earlier review runs also encountered clipboard contention. Temporary test files stayed in the workspace.
- `npm run tauri build`: produced both ARM64 NSIS and MSI installers for 0.1.1.
- `npm audit`: zero reported advisories. Tantivy 0.27 uses `lru 0.18.5`, removing both reviewed `lru` warnings. The lockfile retains the upstream non-Windows `glib` warning and the `instant`/`proc-macro-error` maintenance notices. The `glib` chain is not part of the active Windows build; this update does not claim to repair GTK dependencies for a future Linux port.

### Real Windows WebView2 results

`scripts/verify-desktop.mjs` launched a copy of the built release executable with separate WebView2 storage, separate app data, and disposable vaults. Ten checks passed across the full run and the native-close-only run:

1. Inline/display math and top-level/quoted Mermaid render with CSP enabled.
2. The actual Windows Recycle Bin lists and restores a disposable note to its original path.
3. The daily-note shortcut creates a note from its configured template.
4. Searching `recieve` finds a note containing `receive`.
5. CSS snippet toggles apply and the native summon shortcut registers.
6. The editor exposes `spellcheck=true`.
7. Dark-mode print content uses dark diagram labels on white, with no export document styles inserted into the app.
8. A read-only destination causes an actual atomic-write failure; native close retains the edits. Retrying after clearing read-only saves and hides the resident window.
9. Quit remains alive beyond five seconds when IPC save acknowledgements are delayed, then exits with edits on disk. This simulates a slow acknowledgement; it does not measure physical disk latency.
10. Nonresident native close saves a last-moment edit before exiting.

No JavaScript console errors occurred in the passing runs. Evidence is in `src-tauri/target/desktop-verification/1792409653643/` and `src-tauri/target/desktop-verification/1792409664180/`, including `results.json`, screenshots, and isolated fixture files. The test method follows [Playwright's WebView2 guide](https://playwright.dev/docs/webview2).

Remaining manual checks are Windows dictionary underlines/correction menus, delivery of the physical global hotkey from another app, the printer dialog and saved PDF, and installation over 0.1.0. See `MANUAL_TESTS.md`. The installers were built; they were not installed over the user's existing installation.
