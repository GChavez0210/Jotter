<script lang="ts">
  import { onMount } from 'svelte';
  import Editor from './lib/Editor.svelte';
  import { fluentIcons } from './lib/icons';
  import {
    getBackdropState,
    previewClipboardPayload,
    syncWindowTheme,
  } from './lib/ipc';
  import type {
    BackdropState,
    ClipboardPayload,
    CopyMode,
    ThemePreference,
  } from './lib/types';

  const SAMPLE_NOTE = `# Jotter Clipboard Spike — Line Break Verification

Copy any block below with Ctrl+C (or the Copy button) and paste it into Google Chat, Slack, Gmail, Microsoft Word, or Windows Notepad.

## 1. Consecutive Single Line Breaks
Line A: First line with **bold text** and *italic text*.
Line B: Immediately below Line A (single newline, never collapsed into a space).
Line C: Immediately below Line B with \`inline code\` and ~~strikethrough~~.

## 2. Preserved Blank Lines (Single and Double Gaps)
Paragraph 1: Separated from Paragraph 2 by one blank line.

Paragraph 2: Separated from Paragraph 3 by two blank lines.


Paragraph 3: Exact blank line count stays intact just like Windows Notepad.

## 3. Obsidian Vault Syntax & Lists
See [[Product Spec|Jotter Product Spec]] and [[Architecture#Clipboard Engine]] for #windows11 details.
- [x] Write text/plain with real \\n line breaks
- [x] Write text/html where every newline becomes <br>
- [ ] Run manual paste checks in target apps
> Tip: Press Ctrl+Shift+V anywhere in the editor to paste clipboard contents strictly as plain text.`;

  const COPY_MODE_LABELS: Record<CopyMode, { label: string; desc: string }> = {
    plain: {
      label: 'Plain',
      desc: 'Plain text only (Markdown stripped), exact line breaks preserved',
    },
    rich: {
      label: 'Rich',
      desc: 'HTML (<br> newlines + formatting) plus plain-text alternative',
    },
    markdown_source: {
      label: 'Markdown source',
      desc: 'Raw Markdown syntax with exact line breaks preserved',
    },
  };

  let copyMode: CopyMode = $state('plain');
  let themePref: ThemePreference = $state('system');
  let systemIsDark = $state(false);
  let forceSolidBackdrop = $state(false);
  let backdropState: BackdropState = $state({
    micaApplied: true,
    backdropKind: 'mica',
    darkMode: null,
  });

  let showSettings = $state(false);
  let showInspector = $state(true);
  let lastPayload: ClipboardPayload | null = $state(null);
  let statusBanner = $state('Ready — Ctrl+C copies with arboard set_html + plain fallback');
  let statusTimestamp = $state('');

  let docStats = $state({
    lines: 0,
    words: 0,
    chars: 0,
    selectedChars: 0,
    selectedLines: 0,
  });

  let editorRef: Editor | undefined = $state();

  let effectiveDark = $derived(
    themePref === 'system' ? systemIsDark : themePref === 'dark'
  );

  let effectiveSolidBackdrop = $derived(
    forceSolidBackdrop || !backdropState.micaApplied
  );

  function setCopyMode(next: CopyMode) {
    copyMode = next;
    try {
      localStorage.setItem('jotter.copyMode', next);
    } catch {
      // ignore storage errors
    }
    void refreshInspectorPreview();
  }

  function setThemePreference(next: ThemePreference) {
    themePref = next;
    try {
      localStorage.setItem('jotter.themePref', next);
    } catch {
      // ignore
    }
    applyThemeToDocument(next, systemIsDark);
  }

  function applyThemeToDocument(pref: ThemePreference, sysDark: boolean) {
    const root = document.documentElement;
    if (pref === 'system') {
      root.removeAttribute('data-theme');
    } else {
      root.setAttribute('data-theme', pref);
    }
    const isDark = pref === 'system' ? sysDark : pref === 'dark';
    void syncWindowTheme(isDark)
      .then((res) => {
        backdropState = res;
      })
      .catch(() => {
        // Fallback when running in pure Vite browser preview outside Tauri
        backdropState = {
          micaApplied: false,
          backdropKind: 'solid_fallback',
          darkMode: isDark,
        };
      });
  }

  async function refreshInspectorPreview() {
    const sample =
      editorRef?.getCurrentCopyTargetText() ??
      'Line 1 with **bold**\nLine 2 right below\n\nLine 4 after blank line';
    try {
      lastPayload = await previewClipboardPayload({
        text: sample,
        mode: copyMode,
      });
    } catch {
      // Ignore in browser-only preview
    }
  }

  function handleCopyComplete(payload: ClipboardPayload, source: 'copy' | 'cut') {
    lastPayload = payload;
    const verb = source === 'cut' ? 'Cut' : 'Copied';
    const modeLabel = COPY_MODE_LABELS[payload.mode].label;
    statusBanner = `${verb} ${payload.lineCount} ${
      payload.lineCount === 1 ? 'line' : 'lines'
    } (${payload.charCount} chars) in ${modeLabel} mode`;
    statusTimestamp = new Date().toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  }

  function handlePastePlainComplete(charCount: number, lineCount: number) {
    statusBanner = `Pasted ${lineCount} ${
      lineCount === 1 ? 'line' : 'lines'
    } (${charCount} chars) as plain text (Ctrl+Shift+V)`;
    statusTimestamp = new Date().toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  }

  function handleError(err: string) {
    statusBanner = `Clipboard error: ${err}`;
  }

  onMount(() => {
    try {
      const savedMode = localStorage.getItem('jotter.copyMode') as CopyMode | null;
      if (
        savedMode === 'plain' ||
        savedMode === 'rich' ||
        savedMode === 'markdown_source'
      ) {
        copyMode = savedMode;
      }
      const savedTheme = localStorage.getItem(
        'jotter.themePref'
      ) as ThemePreference | null;
      if (
        savedTheme === 'system' ||
        savedTheme === 'light' ||
        savedTheme === 'dark'
      ) {
        themePref = savedTheme;
      }
    } catch {
      // ignore
    }

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    systemIsDark = mediaQuery.matches;
    applyThemeToDocument(themePref, systemIsDark);

    const handleMediaChange = (e: MediaQueryListEvent) => {
      systemIsDark = e.matches;
      applyThemeToDocument(themePref, systemIsDark);
    };
    mediaQuery.addEventListener('change', handleMediaChange);

    void getBackdropState()
      .then((s) => {
        backdropState = s;
      })
      .catch(() => {
        backdropState = {
          micaApplied: false,
          backdropKind: 'solid_fallback',
          darkMode: systemIsDark,
        };
      });

    void refreshInspectorPreview();

    return () => {
      mediaQuery.removeEventListener('change', handleMediaChange);
    };
  });
</script>

<div
  class="app-shell"
  class:solid-backdrop={effectiveSolidBackdrop}
  data-backdrop={effectiveSolidBackdrop ? 'solid' : 'mica'}
>
  <!-- Integrated Fluent Header & Toolbar -->
  <header class="command-bar" aria-label="Application toolbar">
    <div class="brand-group">
      <span class="brand-icon">{@html fluentIcons.note20}</span>
      <span class="brand-title">Jotter</span>
      <span class="note-pill" title="Active spike note">Clipboard Spike.md</span>
    </div>

    <div class="toolbar-actions">
      <!-- Copy Mode Selector (Plain / Rich / Markdown source) -->
      <div class="segmented-group" role="radiogroup" aria-label="Copy mode">
        <span class="segmented-label">Copy mode:</span>
        {#each ['plain', 'rich', 'markdown_source'] as const as mode}
          <button
            type="button"
            role="radio"
            aria-checked={copyMode === mode}
            class="seg-btn"
            class:active={copyMode === mode}
            title={COPY_MODE_LABELS[mode].desc}
            onclick={() => setCopyMode(mode)}
          >
            {COPY_MODE_LABELS[mode].label}
          </button>
        {/each}
      </div>

      <div class="divider" role="separator"></div>

      <button
        type="button"
        class="fluent-btn primary-btn"
        title="Copy selection or entire note using custom dual-format clipboard handler (Ctrl+C)"
        onclick={() => editorRef?.triggerCopyFromToolbar()}
      >
        {@html fluentIcons.copy16}
        <span>Copy</span>
      </button>

      <button
        type="button"
        class="fluent-btn"
        title="Paste clipboard contents strictly as plain text (Ctrl+Shift+V)"
        onclick={() => editorRef?.triggerPastePlainFromToolbar()}
      >
        {@html fluentIcons.clipboardPaste16}
        <span>Paste Plain</span>
      </button>

      <button
        type="button"
        class="fluent-btn"
        class:active-toggle={showInspector}
        aria-pressed={showInspector}
        title="Toggle live clipboard payload inspector (text/plain and text/html)"
        onclick={() => {
          showInspector = !showInspector;
          if (showInspector) void refreshInspectorPreview();
        }}
      >
        {@html fluentIcons.code16}
        <span>Inspector</span>
      </button>

      <div class="settings-wrapper">
        <button
          type="button"
          class="fluent-btn icon-only"
          aria-expanded={showSettings}
          aria-label="Clipboard and appearance settings"
          title="Settings"
          onclick={() => (showSettings = !showSettings)}
        >
          {@html fluentIcons.settings16}
        </button>

        {#if showSettings}
          <div class="settings-popover" role="dialog" aria-label="Settings">
            <div class="popover-section">
              <div class="popover-heading">Copy Format Mode</div>
              {#each ['plain', 'rich', 'markdown_source'] as const as mode}
                <label class="radio-row">
                  <input
                    type="radio"
                    name="copy-mode-setting"
                    checked={copyMode === mode}
                    onchange={() => setCopyMode(mode)}
                  />
                  <div class="radio-text">
                    <span class="radio-title">{COPY_MODE_LABELS[mode].label}</span>
                    <span class="radio-desc">{COPY_MODE_LABELS[mode].desc}</span>
                  </div>
                </label>
              {/each}
            </div>

            <div class="popover-section">
              <div class="popover-heading">Theme</div>
              <div class="theme-pills">
                {#each ['system', 'light', 'dark'] as const as pref}
                  <button
                    type="button"
                    class="seg-btn"
                    class:active={themePref === pref}
                    onclick={() => setThemePreference(pref)}
                  >
                    {pref === 'system'
                      ? 'System'
                      : pref === 'light'
                        ? 'Light'
                        : 'Dark'}
                  </button>
                {/each}
              </div>
            </div>

            <div class="popover-section">
              <div class="popover-heading">Window Backdrop</div>
              <label class="checkbox-row">
                <input
                  type="checkbox"
                  bind:checked={forceSolidBackdrop}
                />
                <span>Use solid-color fallback (Win10 / transparency off)</span>
              </label>
            </div>
          </div>
        {/if}
      </div>
    </div>
  </header>

  <!-- Main Workspace Area -->
  <main class="workspace">
    <section class="editor-card" aria-label="Editor pane">
      <Editor
        bind:this={editorRef}
        initialDoc={SAMPLE_NOTE}
        {copyMode}
        onCopyComplete={handleCopyComplete}
        onPastePlainComplete={handlePastePlainComplete}
        onStatsChange={(s) => (docStats = s)}
        onError={handleError}
      />
    </section>

    {#if showInspector && lastPayload}
      <aside class="inspector-card" aria-label="Clipboard format inspector">
        <div class="inspector-header">
          <div class="inspector-title">
            {@html fluentIcons.checkmark16}
            <span>
              arboard::Clipboard::set_html Dual-Format Output ({COPY_MODE_LABELS[
                lastPayload.mode
              ].label})
            </span>
          </div>
          <span class="inspector-meta">
            {lastPayload.lineCount} lines · {lastPayload.charCount} chars
          </span>
        </div>

        <div class="inspector-grid">
          <div class="format-box">
            <div class="format-label">
              <span>text/plain (real <code>\n</code> line breaks)</span>
            </div>
            <pre class="format-code">{lastPayload.plain}</pre>
          </div>

          <div class="format-box">
            <div class="format-label">
              <span>text/html (every newline becomes <code>&lt;br&gt;</code>)</span>
            </div>
            <pre class="format-code">{lastPayload.html}</pre>
          </div>
        </div>
      </aside>
    {/if}
  </main>

  <!-- Fluent Status Bar -->
  <footer class="status-bar" aria-label="Status bar">
    <div class="status-left">
      <span class="status-indicator">{statusBanner}</span>
      {#if statusTimestamp}
        <span class="status-time">({statusTimestamp})</span>
      {/if}
    </div>

    <div class="status-right">
      <span class="status-item" title="Active window backdrop">
        Backdrop: {effectiveSolidBackdrop ? 'Solid Fallback' : 'Windows 11 Mica'}
      </span>
      <span class="status-item" title="Active color theme">
        Theme: {effectiveDark ? 'Dark' : 'Light'} ({themePref})
      </span>
      <span class="status-item">
        {#if docStats.selectedChars > 0}
          Selected: {docStats.selectedLines} ln, {docStats.selectedChars} ch ·
        {/if}
        {docStats.lines} ln · {docStats.words} words
      </span>
    </div>
  </footer>
</div>

<style>
  .app-shell {
    display: flex;
    flex-direction: column;
    width: 100vw;
    height: 100vh;
    background-color: var(--bg-mica-tint);
    transition: background-color var(--transition-fast);
  }

  .app-shell.solid-backdrop {
    background-color: var(--bg-solid-fallback);
  }

  .command-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--border-subtle);
    user-select: none;
    flex-shrink: 0;
  }

  .brand-group {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .brand-icon {
    display: inline-flex;
    align-items: center;
    color: var(--accent-color, var(--accent-fallback));
  }

  .brand-title {
    font-family: var(--font-display);
    font-size: var(--text-base);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .note-pill {
    font-size: var(--text-xs);
    color: var(--text-secondary);
    background: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-control);
    white-space: nowrap;
  }

  .toolbar-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .segmented-group {
    display: inline-flex;
    align-items: center;
    background: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-control);
    padding: 2px;
    gap: 2px;
  }

  .segmented-label {
    font-size: var(--text-xs);
    color: var(--text-secondary);
    padding: 0 var(--space-2);
  }

  .seg-btn {
    min-height: 28px;
    padding: 0 var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font-size: var(--text-xs);
    font-weight: 500;
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      color var(--transition-fast),
      border-color var(--transition-fast);
  }

  .seg-btn:hover {
    background: var(--bg-control-hover);
    color: var(--text-primary);
  }

  .seg-btn.active {
    background: var(--bg-card-solid);
    color: var(--text-primary);
    border-color: var(--border-control);
    font-weight: 600;
  }

  .divider {
    width: 1px;
    height: 20px;
    background: var(--border-subtle);
    margin: 0 var(--space-1);
  }

  .fluent-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    min-height: var(--control-min-height);
    min-width: var(--control-min-height);
    padding: 0 var(--space-3);
    border-radius: var(--radius-control);
    border: 1px solid var(--border-control);
    background: var(--bg-control);
    color: var(--text-primary);
    font-size: var(--text-xs);
    font-weight: 500;
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      border-color var(--transition-fast);
  }

  .fluent-btn.icon-only {
    padding: 0 var(--space-2);
  }

  .fluent-btn:hover {
    background: var(--bg-control-hover);
  }

  .fluent-btn:active {
    background: var(--bg-control-active);
  }

  .fluent-btn.active-toggle {
    border-color: var(--accent-color, var(--accent-fallback));
    background: var(--selection-bg);
  }

  .fluent-btn.primary-btn {
    background: var(--accent-color, var(--accent-fallback));
    color: var(--accent-text, #ffffff);
    border-color: transparent;
  }

  .fluent-btn.primary-btn:hover {
    filter: brightness(1.06);
  }

  .settings-wrapper {
    position: relative;
  }

  .settings-popover {
    position: absolute;
    top: calc(100% + var(--space-2));
    right: 0;
    width: 320px;
    background: var(--bg-card-solid);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-card);
    padding: var(--space-4);
    z-index: 50;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .popover-section {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .popover-heading {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .radio-row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-2);
    border-radius: var(--radius-control);
    cursor: pointer;
  }

  .radio-row:hover {
    background: var(--bg-control-hover);
  }

  .radio-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .radio-title {
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .radio-desc {
    font-size: var(--text-xs);
    color: var(--text-secondary);
  }

  .theme-pills {
    display: flex;
    gap: var(--space-1);
    background: var(--bg-subtle);
    padding: 2px;
    border-radius: var(--radius-control);
    border: 1px solid var(--border-subtle);
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-min-height);
    font-size: var(--text-xs);
    cursor: pointer;
  }

  .workspace {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    min-height: 0;
    overflow: hidden;
  }

  .editor-card {
    flex: 1;
    min-height: 0;
    background: var(--bg-card);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-card);
    overflow: hidden;
    transition: background-color var(--transition-fast);
  }

  .solid-backdrop .editor-card {
    background: var(--bg-card-solid);
  }

  .inspector-card {
    height: 188px;
    flex-shrink: 0;
    background: var(--bg-card);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-card);
    padding: var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    overflow: hidden;
  }

  .solid-backdrop .inspector-card {
    background: var(--bg-card-solid);
  }

  .inspector-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: var(--text-xs);
  }

  .inspector-title {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: 600;
    color: var(--status-ok);
  }

  .inspector-meta {
    color: var(--text-secondary);
  }

  .inspector-grid {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
    min-height: 0;
  }

  .format-box {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-height: 0;
    background: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-control);
    padding: var(--space-2);
  }

  .format-label {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-secondary);
  }

  .format-label code {
    font-family: var(--font-mono);
    background: var(--bg-code);
    padding: 1px 4px;
    border-radius: var(--radius-control);
  }

  .format-code {
    margin: 0;
    flex: 1;
    overflow: auto;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--text-primary);
  }

  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-1) var(--space-4);
    min-height: 28px;
    border-top: 1px solid var(--border-subtle);
    font-size: var(--text-xs);
    color: var(--text-secondary);
    user-select: none;
    flex-shrink: 0;
  }

  .status-left,
  .status-right {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .status-indicator {
    color: var(--text-primary);
    font-weight: 500;
  }

  .status-time {
    color: var(--text-muted);
  }

  .status-item {
    white-space: nowrap;
  }
</style>
