<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './ui/Icon.svelte';
  import Button from './ui/Button.svelte';
  import IconButton from './ui/IconButton.svelte';
  import Segmented from './ui/Segmented.svelte';
  import Dialog from './ui/Dialog.svelte';
  import { COPY_MODE_LABELS, settingsStore } from './stores/settings.svelte';
  import { ACCENT_PRESETS } from './designTokens';
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

<Dialog label="Settings" variant="panel" {onClose}>
    <div class="settings-body">
      <nav class="settings-nav" aria-label="Settings categories">
        <h2 class="nav-title">Settings</h2>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'editor'}
          aria-current={activeSection === 'editor' ? 'page' : undefined}
          onclick={() => (activeSection = 'editor')}
        >
          <Icon name="pencil-simple" />
          <span>Editor</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'copy'}
          aria-current={activeSection === 'copy' ? 'page' : undefined}
          onclick={() => (activeSection = 'copy')}
        >
          <Icon name="copy" />
          <span>Copy</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'appearance'}
          aria-current={activeSection === 'appearance' ? 'page' : undefined}
          onclick={() => (activeSection = 'appearance')}
        >
          <Icon name="eye" />
          <span>Appearance</span>
        </button>
        <button
          type="button"
          class="nav-item"
          class:active={activeSection === 'vault'}
          aria-current={activeSection === 'vault' ? 'page' : undefined}
          onclick={() => (activeSection = 'vault')}
        >
          <Icon name="folder-simple" />
          <span>Vault</span>
        </button>
      </nav>

      <div class="settings-content">
        <div class="close-btn">
          <IconButton
            bind:ref={closeBtnRef}
            icon="x"
            label="Close settings"
            tooltip="Close settings (Esc)"
            onclick={onClose}
          />
        </div>
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
              <Segmented
                label="Default editing mode"
                value={settingsStore.editingMode}
                options={[
                  { value: 'live', label: 'Live Preview' },
                  { value: 'source', label: 'Source' },
                ]}
                onchange={(mode) => settingsStore.setEditingMode(mode)}
              />
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
                  Size of note text: 12, 14, 16 or 20 px.
                </div>
              </div>
              <Segmented
                label="Editor font size"
                value={settingsStore.editorFontSize}
                options={[12, 14, 16, 20].map((px) => ({ value: px, label: `${px} px` }))}
                onchange={(px) => settingsStore.setEditorFontSize(px)}
              />
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">Pasted image link format</div>
                <div class="setting-desc">
                  Choose how images pasted from the clipboard into the <code>attachments/</code> folder are inserted into the note.
                </div>
              </div>
              <Segmented
                label="Pasted image link format"
                value={settingsStore.imageLinkFormat}
                options={[
                  { value: 'wikilink', label: '![[name]]' },
                  { value: 'markdown', label: '![](path)' },
                ]}
                onchange={(format) => settingsStore.setImageLinkFormat(format)}
              />
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
              Theme, accent color and window backdrop.
            </p>

            <div class="setting-card">
              <div class="setting-info">
                <div class="setting-label">Theme mode</div>
                <div class="setting-desc">
                  Follow your system setting, or always use Light or Dark.
                </div>
              </div>
              <Segmented
                label="Theme mode"
                value={settingsStore.themePref}
                options={[
                  { value: 'system', label: 'System' },
                  { value: 'light', label: 'Light' },
                  { value: 'dark', label: 'Dark' },
                ]}
                onchange={(pref) => settingsStore.setThemePreference(pref)}
              />
            </div>

            <div class="setting-card">
              <div class="setting-info">
                <label for="toggle-mica" class="setting-label">
                  Mica backdrop (Windows 11)
                </label>
                <div class="setting-desc">
                  Let the desktop show through the window background. Off by default; Jotter uses solid paper surfaces instead.
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
                    Used for links, the active note, selection and primary buttons.
                  </div>
                </div>
                <Segmented
                  label="Accent color source"
                  value={settingsStore.accentColorMode}
                  options={[
                    { value: 'preset', label: 'Jotter' },
                    { value: 'system', label: 'System' },
                    { value: 'custom', label: 'Custom' },
                  ]}
                  onchange={(mode) => settingsStore.setAccentColor(mode)}
                />
              </div>

              {#if settingsStore.accentColorMode === 'preset'}
                <div class="accent-swatches" role="radiogroup" aria-label="Accent preset">
                  {#each ACCENT_PRESETS as swatch}
                    <button
                      type="button"
                      role="radio"
                      class="swatch-btn"
                      class:active={settingsStore.accentPreset === swatch.id}
                      aria-checked={settingsStore.accentPreset === swatch.id}
                      style:background-color={swatch.light}
                      title={swatch.label}
                      aria-label={swatch.label}
                      onclick={() => settingsStore.setAccentColor('preset', swatch.id)}
                    ></button>
                  {/each}
                </div>
              {:else if settingsStore.accentColorMode === 'custom'}
                <div class="accent-swatches">
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
              <Button
                variant="primary"
                icon="arrow-clockwise"
                disabled={!vaultStore.vaultSnapshot || isRebuilding}
                onclick={() => void handleRebuildIndex()}
              >
                {isRebuilding ? 'Rebuilding…' : 'Rebuild index'}
              </Button>
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
</Dialog>

<style>
  .settings-body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .settings-nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 188px;
    flex: none;
    padding: var(--space-4) var(--space-2);
    border-right: 1px solid var(--line-subtle);
    background: var(--surface-app);
  }

  .nav-title {
    margin: var(--space-1) var(--space-2) var(--space-3);
    font: 600 var(--fs-17) / 1.2 var(--font-display);
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--row-height);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .nav-item:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .nav-item.active {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 500;
  }

  .settings-content {
    position: relative;
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: var(--space-5) var(--space-6) var(--space-8);
  }

  .close-btn {
    position: absolute;
    top: var(--space-3);
    right: var(--space-3);
  }

  .settings-section {
    display: flex;
    flex-direction: column;
  }

  .section-title {
    margin: 0 0 var(--space-1);
    font: 600 var(--fs-24) / 1.25 var(--font-display);
  }

  .section-subtitle {
    margin: 0 0 var(--space-2);
    color: var(--text-muted);
    font-size: var(--fs-13);
  }

  .setting-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-6);
    padding: var(--space-4) 0;
    border-bottom: 1px solid var(--line-subtle);
  }

  .setting-card:last-child {
    border-bottom: 0;
  }

  .setting-card.column-card {
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-3);
  }

  .setting-row-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-6);
  }

  .setting-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .setting-label {
    color: var(--text-primary);
    font-weight: 600;
  }

  .setting-desc {
    color: var(--text-muted);
    font-size: var(--fs-12);
    line-height: var(--leading-normal);
  }

  .setting-desc code {
    font-family: var(--font-mono);
    font-size: 0.95em;
  }

  .rebuild-status {
    margin-top: var(--space-1);
    color: var(--status-ok);
    font-size: var(--fs-12);
  }

  /* Switch built on a native checkbox so keyboard and screen readers work unchanged. */
  .fluent-toggle {
    appearance: none;
    position: relative;
    width: 36px;
    height: 20px;
    flex: none;
    margin: 0;
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    background: var(--surface-field);
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      border-color var(--transition-fast);
  }

  .fluent-toggle::before {
    content: '';
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--text-secondary);
    transition: transform var(--transition-fast);
  }

  .fluent-toggle:checked {
    border-color: var(--accent);
    background: var(--accent);
  }

  .fluent-toggle:checked::before {
    transform: translateX(16px);
    background: var(--on-accent);
  }

  @media (forced-colors: active) {
    .fluent-toggle {
      appearance: auto;
    }

    .fluent-toggle::before {
      display: none;
    }
  }

  .copy-modes-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }

  .copy-mode-card {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-card);
    cursor: pointer;
  }

  .copy-mode-card:hover {
    border-color: var(--line-control);
  }

  .copy-mode-card.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }

  .copy-mode-card input {
    margin-top: 3px;
    accent-color: var(--accent);
  }

  .copy-mode-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .copy-mode-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .copy-mode-title {
    font-weight: 600;
  }

  .copy-mode-badge {
    padding: 1px 6px;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: var(--fs-11);
    font-weight: 500;
  }

  .copy-mode-formats {
    color: var(--text-secondary);
    font-family: var(--font-mono);
    font-size: var(--fs-12);
  }

  .copy-mode-desc {
    color: var(--text-muted);
    font-size: var(--fs-12);
    line-height: var(--leading-normal);
  }

  .accent-swatches {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
  }

  .swatch-btn {
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    box-shadow: 0 0 0 1px var(--line-subtle);
    cursor: pointer;
  }

  .swatch-btn.active {
    box-shadow:
      0 0 0 2px var(--surface-raised),
      0 0 0 4px var(--text-primary);
  }

  .color-picker-label {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-secondary);
    font-size: var(--fs-13);
  }
</style>
