<script lang="ts">
  import { onMount } from 'svelte';
  import { fluentIcons } from './icons';
  import { COPY_MODE_LABELS, settingsStore } from './stores/settings.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import type { CopyMode } from './types';

  interface Props {
    onClose: () => void;
    onRefreshInspector: () => void;
  }

  let { onClose, onRefreshInspector }: Props = $props();

  type SettingsSection = 'editor' | 'copy' | 'appearance' | 'vault';
  let activeSection = $state<SettingsSection>('editor');
  let rebuildStatus = $state('');
  let isRebuilding = $state(false);
  let closeBtnRef: HTMLButtonElement | undefined = $state();

  const ACCENT_PRESETS = [
    { label: 'Fluent Blue', hex: '#0067c0' },
    { label: ' Teal', hex: '#038387' },
    { label: 'Forest Green', hex: '#0f7b0f' },
    { label: 'Iris Purple', hex: '#8764b8' },
    { label: 'Crimson', hex: '#c42b1c' },
    { label: 'Amber', hex: '#ca5010' },
  ];

  const COPY_MODE_DETAILS: Record<
    CopyMode,
    { badge: string; formats: string; details: string }
  > = {
    markdown_source: {
      badge: 'Default',
      formats: 'Writes text/plain + text/html (<br> & &nbsp;)',
      details:
        'Preserves raw Markdown syntax and exact Notepad line breaks and indentation across Google Chat, Slack, Gmail, Word, and Notepad.',
    },
    plain: {
      badge: 'Opt-in Clean Text',
      formats: 'Writes text/plain ONLY (no CF_HTML)',
      details:
        'Strips Markdown markers (#, **, *, ~~, `, [label](url), [[Target|Alias]] outside inline code) and drops ``` / ~~~ fence lines so line counts match.',
    },
    rich: {
      badge: 'Formatted HTML',
      formats: 'Writes text/html (<br> & &nbsp;) + text/plain',
      details:
        'Renders inline Markdown (bold, italic, code, links) via comrak, drops code fences, preserves leading/repeated spaces with &nbsp;, and joins lines strictly with <br> instead of <p> margins.',
    },
  };

  function handleSelectCopyMode(mode: CopyMode) {
    settingsStore.setCopyMode(mode, () => {
      if (settingsStore.showInspector) onRefreshInspector();
    });
  }

  async function handleRebuildIndex() {
    if (!vaultStore.vaultSnapshot || isRebuilding) return;
    isRebuilding = true;
    rebuildStatus = 'Rebuilding SQLite and Tantivy indexes...';
    const count = await vaultStore.rebuildIndex();
    isRebuilding = false;
    if (count !== null) {
      rebuildStatus = `Rebuilt index for ${count} notes (in-app edit_log preserved).`;
    } else {
      rebuildStatus = 'Failed to rebuild vault index.';
    }
  }

  onMount(() => {
    closeBtnRef?.focus();
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="settings-backdrop"
  onmousedown={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
>
  <div
    class="settings-dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="settings-dialog-title"
  >
    <header class="settings-header">
      <div class="header-title-group">
        <span class="header-icon">{@html fluentIcons.settings16}</span>
        <h2 id="settings-dialog-title" class="header-title">Settings</h2>
        <span class="shortcut-pill">Ctrl+,</span>
      </div>
      <button
        bind:this={closeBtnRef}
        type="button"
        class="fluent-btn icon-only"
        aria-label="Close settings (Escape)"
        title="Close settings (Escape)"
        onclick={onClose}
      >
        {@html fluentIcons.dismiss16}
      </button>
    </header>

    <div class="settings-body">
      <nav class="settings-nav" aria-label="Settings categories">
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'editor'}
          aria-current={activeSection === 'editor' ? 'page' : undefined}
          onclick={() => (activeSection = 'editor')}
        >
          {@html fluentIcons.edit16}
          <span>Editor</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'copy'}
          aria-current={activeSection === 'copy' ? 'page' : undefined}
          onclick={() => (activeSection = 'copy')}
        >
          {@html fluentIcons.copy16}
          <span>Copy</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'appearance'}
          aria-current={activeSection === 'appearance' ? 'page' : undefined}
          onclick={() => (activeSection = 'appearance')}
        >
          {@html fluentIcons.eye16}
          <span>Appearance</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'vault'}
          aria-current={activeSection === 'vault' ? 'page' : undefined}
          onclick={() => (activeSection = 'vault')}
        >
          {@html fluentIcons.folder16}
          <span>Vault</span>
        </button>
      </nav>

      <div class="settings-content">
        {#if activeSection === 'editor'}
          <section class="settings-section" aria-labelledby="sec-editor">
            <h3 id="sec-editor" class="section-title">Editor</h3>
            <p class="section-subtitle">
              Configure default editing mode, focus aids, font scale, and image paste syntax.
            </p>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">Default editing mode</div>
                <div class="setting-desc">
                  Live mode renders headings, emphasis, links, task checkboxes, and images inline while keeping raw Markdown on the active cursor line (Ctrl+/).
                </div>
              </div>
              <div class="segmented-group" role="radiogroup" aria-label="Default editing mode">
                <button
                  type="button"
                  role="radio"
                  aria-checked={settingsStore.editingMode === 'live'}
                  class="seg-btn"
                  class:active={settingsStore.editingMode === 'live'}
                  onclick={() => settingsStore.setEditingMode('live')}
                >
                  Live Preview
                </button>
                <button
                  type="button"
                  role="radio"
                  aria-checked={settingsStore.editingMode === 'source'}
                  class="seg-btn"
                  class:active={settingsStore.editingMode === 'source'}
                  onclick={() => settingsStore.setEditingMode('source')}
                >
                  Source
                </button>
              </div>
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-typewriter" class="setting-label">
                  Typewriter scrolling
                </label>
                <div class="setting-desc">
                  Keep the active cursor line vertically centered in the editor viewport while typing.
                </div>
              </div>
              <input
                id="toggle-typewriter"
                type="checkbox"
                class="fluent-toggle"
                checked={settingsStore.typewriterScrolling}
                onchange={(e) =>
                  settingsStore.setTypewriterScrolling(
                    (e.currentTarget as HTMLInputElement).checked
                  )}
              />
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-dimming" class="setting-label">
                  Focus paragraph dimming
                </label>
                <div class="setting-desc">
                  Dim all lines outside the current paragraph to help focus on the active block.
                </div>
              </div>
              <input
                id="toggle-dimming"
                type="checkbox"
                class="fluent-toggle"
                checked={settingsStore.dimInactiveParagraphs}
                onchange={(e) =>
                  settingsStore.setDimInactiveParagraphs(
                    (e.currentTarget as HTMLInputElement).checked
                  )}
              />
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">Editor font size</div>
                <div class="setting-desc">
                  Select a step on the Fluent typography scale (12, 14, 16, or 20 px).
                </div>
              </div>
              <div class="segmented-group" role="radiogroup" aria-label="Editor font size">
                {#each [12, 14, 16, 20] as sizePx}
                  <button
                    type="button"
                    role="radio"
                    aria-checked={settingsStore.editorFontSize === sizePx}
                    class="seg-btn"
                    class:active={settingsStore.editorFontSize === sizePx}
                    onclick={() => settingsStore.setEditorFontSize(sizePx)}
                  >
                    {sizePx} px
                  </button>
                {/each}
              </div>
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">Pasted image link format</div>
                <div class="setting-desc">
                  Choose how images pasted from the clipboard into the <code>attachments/</code> folder are inserted into the note.
                </div>
              </div>
              <div class="segmented-group" role="radiogroup" aria-label="Pasted image link format">
                <button
                  type="button"
                  role="radio"
                  aria-checked={settingsStore.imageLinkFormat === 'wikilink'}
                  class="seg-btn"
                  class:active={settingsStore.imageLinkFormat === 'wikilink'}
                  onclick={() => settingsStore.setImageLinkFormat('wikilink')}
                >
                  ![[name]]
                </button>
                <button
                  type="button"
                  role="radio"
                  aria-checked={settingsStore.imageLinkFormat === 'markdown'}
                  class="seg-btn"
                  class:active={settingsStore.imageLinkFormat === 'markdown'}
                  onclick={() => settingsStore.setImageLinkFormat('markdown')}
                >
                  ![](path)
                </button>
              </div>
            </div>
          </section>
        {:else if activeSection === 'copy'}
          <section class="settings-section" aria-labelledby="sec-copy">
            <h3 id="sec-copy" class="section-title">Copy</h3>
            <p class="section-subtitle">
              Choose the default clipboard copy mode. Every mode guarantees Notepad-identical line breaks when pasting into Google Chat, Slack, Gmail, Word, or Notepad.
            </p>

            <div class="copy-modes-list" role="radiogroup" aria-label="Default copy mode">
              {#each ['markdown_source', 'plain', 'rich'] as const as mode}
                <label
                  class="copy-mode-card"
                  class:selected={settingsStore.copyMode === mode}
                >
                  <input
                    type="radio"
                    name="default-copy-mode"
                    checked={settingsStore.copyMode === mode}
                    onchange={() => handleSelectCopyMode(mode)}
                  />
                  <div class="copy-mode-body">
                    <div class="copy-mode-header">
                      <span class="copy-mode-title">
                        {COPY_MODE_LABELS[mode].label}
                      </span>
                      <span class="copy-mode-badge">
                        {COPY_MODE_DETAILS[mode].badge}
                      </span>
                    </div>
                    <div class="copy-mode-formats">
                      {COPY_MODE_DETAILS[mode].formats}
                    </div>
                    <div class="copy-mode-desc">
                      {COPY_MODE_DETAILS[mode].details}
                    </div>
                  </div>
                </label>
              {/each}
            </div>
          </section>
        {:else if activeSection === 'appearance'}
          <section class="settings-section" aria-labelledby="sec-appearance">
            <h3 id="sec-appearance" class="section-title">Appearance</h3>
            <p class="section-subtitle">
              Customize Windows 11 Fluent theme, Mica window backdrop, and system accent color.
            </p>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">Theme mode</div>
                <div class="setting-desc">
                  Follow the Windows 11 system theme live (`prefers-color-scheme`) or lock Light or Dark mode.
                </div>
              </div>
              <div class="segmented-group" role="radiogroup" aria-label="Theme mode">
                {#each ['system', 'light', 'dark'] as const as pref}
                  <button
                    type="button"
                    role="radio"
                    aria-checked={settingsStore.themePref === pref}
                    class="seg-btn"
                    class:active={settingsStore.themePref === pref}
                    onclick={() => settingsStore.setThemePreference(pref)}
                  >
                    {pref === 'system'
                      ? 'Follow System'
                      : pref === 'light'
                        ? 'Light'
                        : 'Dark'}
                  </button>
                {/each}
              </div>
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-mica" class="setting-label">
                  Windows 11 Mica backdrop
                </label>
                <div class="setting-desc">
                  Use native Mica material on the window background. When off (or on Windows 10), Jotter uses a solid Fluent surface fallback.
                </div>
              </div>
              <input
                id="toggle-mica"
                type="checkbox"
                class="fluent-toggle"
                checked={!settingsStore.forceSolidBackdrop}
                onchange={(e) =>
                  settingsStore.setForceSolidBackdrop(
                    !(e.currentTarget as HTMLInputElement).checked
                  )}
              />
            </div>

            <div class="setting-card column-card">
              <div class="setting-row-top">
                <div class="setting-info">
                  <div class="setting-label">Accent color</div>
                  <div class="setting-desc">
                    Follow the Windows system accent color (`AccentColor`) or choose a custom Fluent accent.
                  </div>
                </div>
                <div class="segmented-group" role="radiogroup" aria-label="Accent color source">
                  <button
                    type="button"
                    role="radio"
                    aria-checked={settingsStore.accentColorMode === 'system'}
                    class="seg-btn"
                    class:active={settingsStore.accentColorMode === 'system'}
                    onclick={() => settingsStore.setAccentColor('system')}
                  >
                    System Accent
                  </button>
                  <button
                    type="button"
                    role="radio"
                    aria-checked={settingsStore.accentColorMode === 'custom'}
                    class="seg-btn"
                    class:active={settingsStore.accentColorMode === 'custom'}
                    onclick={() => settingsStore.setAccentColor('custom')}
                  >
                    Custom
                  </button>
                </div>
              </div>

              {#if settingsStore.accentColorMode === 'custom'}
                <div class="accent-swatches">
                  {#each ACCENT_PRESETS as swatch}
                    <button
                      type="button"
                      class="swatch-btn"
                      class:active={settingsStore.customAccentColor.toLowerCase() ===
                        swatch.hex.toLowerCase()}
                      style:background-color={swatch.hex}
                      title={swatch.label}
                      aria-label="Select accent color {swatch.label}"
                      onclick={() =>
                        settingsStore.setAccentColor('custom', swatch.hex)}
                    ></button>
                  {/each}
                  <label class="color-picker-label" title="Custom hex color">
                    <span>Custom:</span>
                    <input
                      type="color"
                      value={settingsStore.customAccentColor}
                      oninput={(e) =>
                        settingsStore.setAccentColor(
                          'custom',
                          (e.currentTarget as HTMLInputElement).value
                        )}
                    />
                  </label>
                </div>
              {/if}
            </div>
          </section>
        {:else if activeSection === 'vault'}
          <section class="settings-section" aria-labelledby="sec-vault">
            <h3 id="sec-vault" class="section-title">Vault</h3>
            <p class="section-subtitle">
              Manage the derived SQLite & Tantivy indexes and hidden file isolation.
            </p>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">Rebuild vault index</div>
                <div class="setting-desc">
                  {#if vaultStore.vaultSnapshot}
                    Re-scan all <code>.md</code> files in <strong>{vaultStore.vaultSnapshot.name}</strong> ({vaultStore.vaultSnapshot.noteCount} notes) and rebuild the SQLite metadata/links cache and Tantivy search index while preserving <code>edit_log</code>.
                  {:else}
                    Open a vault to rebuild its SQLite and Tantivy indexes.
                  {/if}
                </div>
                {#if rebuildStatus}
                  <div class="rebuild-status" role="status">{rebuildStatus}</div>
                {/if}
              </div>
              <button
                type="button"
                class="fluent-btn primary-btn"
                disabled={!vaultStore.vaultSnapshot || isRebuilding}
                onclick={() => void handleRebuildIndex()}
              >
                {@html fluentIcons.arrowClockwise16}
                <span>{isRebuilding ? 'Rebuilding...' : 'Rebuild Index'}</span>
              </button>
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-hidden-files" class="setting-label">
                  Show hidden files
                </label>
                <div class="setting-desc">
                  Off by default. `.obsidian` is always ignored for Obsidian vault safety.
                </div>
              </div>
              <input
                id="toggle-hidden-files"
                type="checkbox"
                class="fluent-toggle"
                checked={settingsStore.showHiddenFiles}
                onchange={(e) =>
                  settingsStore.setShowHiddenFiles(
                    (e.currentTarget as HTMLInputElement).checked
                  )}
                aria-label="Show hidden files"
              />
            </div>
          </section>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .settings-backdrop {
    position: fixed;
    inset: 0;
    z-index: 220;
    background: rgba(0, 0, 0, 0.42);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-4);
  }

  .settings-dialog {
    width: min(760px, 100%);
    height: min(560px, 90vh);
    background: var(--bg-card-solid);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-card);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .settings-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .header-title-group {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .header-icon {
    display: inline-flex;
    color: var(--accent-color, var(--accent-fallback));
  }

  .header-title {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--text-base);
    font-weight: 600;
  }

  .shortcut-pill {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--text-muted);
    background: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-control);
    padding: 1px 6px;
  }

  .settings-body {
    flex: 1;
    min-height: 0;
    display: flex;
    overflow: hidden;
  }

  .settings-nav {
    width: 180px;
    flex-shrink: 0;
    border-right: 1px solid var(--border-subtle);
    background: var(--bg-subtle);
    padding: var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-min-height);
    padding: 0 var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    transition:
      background-color var(--transition-fast),
      color var(--transition-fast);
  }

  .nav-item:hover {
    background: var(--bg-control-hover);
    color: var(--text-primary);
  }

  .nav-item.active {
    background: var(--bg-card-solid);
    color: var(--text-primary);
    border-color: var(--border-control);
    font-weight: 600;
  }

  .settings-content {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: var(--space-5);
  }

  .settings-section {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .section-title {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--text-lg);
    font-weight: 600;
  }

  .section-subtitle {
    margin: 0 0 var(--space-2) 0;
    font-size: var(--text-xs);
    color: var(--text-secondary);
  }

  .setting-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-4);
    background: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-card);
  }

  .setting-card.column-card {
    flex-direction: column;
    align-items: stretch;
  }

  .setting-row-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .setting-info {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .setting-label {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text-primary);
  }

  .setting-desc {
    font-size: var(--text-xs);
    color: var(--text-secondary);
    line-height: var(--leading-normal);
  }

  .rebuild-status {
    margin-top: var(--space-1);
    font-size: var(--text-xs);
    color: var(--status-ok);
    font-weight: 500;
  }

  .segmented-group {
    display: inline-flex;
    align-items: center;
    background: var(--bg-card-solid);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-control);
    padding: 2px;
    gap: 2px;
    flex-shrink: 0;
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
      color var(--transition-fast);
  }

  .seg-btn:hover {
    background: var(--bg-control-hover);
    color: var(--text-primary);
  }

  .seg-btn.active {
    background: var(--selection-bg);
    color: var(--text-primary);
    border-color: var(--accent-color, var(--accent-fallback));
    font-weight: 600;
  }

  .fluent-toggle {
    width: 18px;
    height: 18px;
    accent-color: var(--accent-color, var(--accent-fallback));
    cursor: pointer;
    flex-shrink: 0;
  }

  .copy-modes-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .copy-mode-card {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    background: var(--bg-subtle);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-card);
    cursor: pointer;
    transition:
      border-color var(--transition-fast),
      background-color var(--transition-fast);
  }

  .copy-mode-card:hover {
    background: var(--bg-control-hover);
  }

  .copy-mode-card.selected {
    border-color: var(--accent-color, var(--accent-fallback));
    background: var(--selection-bg);
  }

  .copy-mode-card input[type='radio'] {
    margin-top: 4px;
    accent-color: var(--accent-color, var(--accent-fallback));
  }

  .copy-mode-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .copy-mode-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .copy-mode-title {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text-primary);
  }

  .copy-mode-badge {
    font-size: var(--text-xs);
    padding: 1px 6px;
    border-radius: var(--radius-control);
    background: var(--bg-card-solid);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
  }

  .copy-mode-formats {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--syn-code);
  }

  .copy-mode-desc {
    font-size: var(--text-xs);
    color: var(--text-secondary);
    line-height: var(--leading-normal);
  }

  .accent-swatches {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding-top: var(--space-2);
    border-top: 1px solid var(--border-subtle);
    flex-wrap: wrap;
  }

  .swatch-btn {
    width: 28px;
    height: 28px;
    border-radius: var(--radius-control);
    border: 2px solid transparent;
    cursor: pointer;
  }

  .swatch-btn.active {
    border-color: var(--text-primary);
    box-shadow: 0 0 0 2px var(--bg-card-solid);
  }

  .color-picker-label {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--text-xs);
    color: var(--text-secondary);
    margin-left: auto;
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
    flex-shrink: 0;
  }

  .fluent-btn.icon-only {
    padding: 0 var(--space-2);
  }

  .fluent-btn:hover:not(:disabled) {
    background: var(--bg-control-hover);
  }

  .fluent-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .fluent-btn.primary-btn {
    background: var(--accent-color, var(--accent-fallback));
    color: var(--accent-text, #ffffff);
    border-color: transparent;
  }
</style>
