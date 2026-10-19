<script lang="ts">
  import { fluentIcons } from './icons';
  import { COPY_MODE_LABELS, settingsStore } from './stores/settings.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import type { CopyMode } from './types';

  interface Props {
    onCopyTrigger: () => void;
    onPastePlainTrigger: () => void;
    onRefreshInspector: () => void;
  }

  let { onCopyTrigger, onPastePlainTrigger, onRefreshInspector }: Props =
    $props();

  function handleSelectCopyMode(mode: CopyMode) {
    settingsStore.setCopyMode(mode, () => {
      if (settingsStore.showInspector) onRefreshInspector();
    });
  }
</script>

{#if settingsStore.focusMode === 'minimal' && !settingsStore.topHoverReveal}
  <!-- Thin, low-contrast title bar in Minimal focus mode -->
  <header class="minimal-title-bar" aria-label="Minimal focus title bar">
    <div class="minimal-left">
      <span class="minimal-brand">Jotter</span>
      {#if tabsStore.activeTab}
        <span class="minimal-sep">/</span>
        <span class="minimal-note">{tabsStore.activeTab.title}</span>
      {:else if vaultStore.vaultSnapshot}
        <span class="minimal-sep">/</span>
        <span class="minimal-note">{vaultStore.vaultSnapshot.name}</span>
      {/if}
    </div>
    <div class="minimal-right">
      <span class="minimal-hint">
        Minimal Focus · Move mouse to top edge for menu · Ctrl+P Commands · Esc to exit
      </span>
      <button
        type="button"
        class="minimal-exit-btn"
        onclick={() => settingsStore.exitFocusMode()}
      >
        Exit (Esc)
      </button>
    </div>
  </header>
{:else if settingsStore.focusMode === 'off' || settingsStore.topHoverReveal}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <header
    class="command-bar"
    class:floating-reveal={settingsStore.focusMode !== 'off' &&
      settingsStore.topHoverReveal}
    aria-label="Application toolbar"
    onmouseleave={() => {
      if (settingsStore.focusMode !== 'off' && !settingsStore.showSettings) {
        settingsStore.topHoverReveal = false;
      }
    }}
  >
    <div class="brand-group">
      {#if vaultStore.vaultSnapshot && settingsStore.focusMode === 'off'}
        <button
          type="button"
          class="fluent-btn icon-only"
          aria-pressed={!settingsStore.sidebarCollapsed}
          title="Toggle file tree sidebar (Ctrl+\)"
          aria-label="Toggle file tree sidebar"
          onclick={() => settingsStore.toggleSidebar()}
        >
          {@html fluentIcons.sidebar16}
        </button>
      {/if}
      <span class="brand-icon">{@html fluentIcons.note20}</span>
      <span class="brand-title">Jotter</span>
      {#if vaultStore.vaultSnapshot}
        <span class="note-pill" title={vaultStore.vaultSnapshot.rootPath}>
          {vaultStore.vaultSnapshot.name}
        </span>
      {:else if tabsStore.scratchpadMode}
        <span class="note-pill">Scratchpad</span>
        <button
          type="button"
          class="fluent-btn"
          onclick={() => (tabsStore.scratchpadMode = false)}
        >
          {@html fluentIcons.folderOpen20}
          <span>Open / Create Vault</span>
        </button>
      {/if}

      {#if settingsStore.focusMode !== 'off'}
        <button
          type="button"
          class="fluent-btn active-toggle"
          title="Exit {settingsStore.focusMode} focus mode (Esc)"
          onclick={() => settingsStore.exitFocusMode()}
        >
          <span>Exit {settingsStore.focusMode === 'zen' ? 'Zen' : 'Minimal'} (Esc)</span>
        </button>
      {/if}
    </div>

    <div class="toolbar-actions">
      <div class="segmented-group" role="radiogroup" aria-label="Copy mode">
        <span class="segmented-label">Copy mode:</span>
        {#each ['markdown_source', 'plain', 'rich'] as const as mode}
          <button
            type="button"
            role="radio"
            aria-checked={settingsStore.copyMode === mode}
            class="seg-btn"
            class:active={settingsStore.copyMode === mode}
            title={COPY_MODE_LABELS[mode].desc}
            onclick={() => handleSelectCopyMode(mode)}
          >
            {COPY_MODE_LABELS[mode].label}
          </button>
        {/each}
      </div>

      <div class="divider" role="separator"></div>

      <button
        type="button"
        class="fluent-btn primary-btn"
        title="Copy selection or note (Ctrl+C)"
        onclick={onCopyTrigger}
      >
        {@html fluentIcons.copy16}
        <span>Copy</span>
      </button>

      <button
        type="button"
        class="fluent-btn"
        title="Paste clipboard contents strictly as plain text (Ctrl+Shift+V)"
        onclick={onPastePlainTrigger}
      >
        {@html fluentIcons.clipboardPaste16}
        <span>Paste Plain</span>
      </button>

      <button
        type="button"
        class="fluent-btn"
        title="Open Command Palette (Ctrl+P)"
        onclick={() => (settingsStore.showCommandPalette = true)}
      >
        {@html fluentIcons.search16}
        <span>Commands</span>
      </button>

      {#if vaultStore.vaultSnapshot}
        <button
          type="button"
          class="fluent-btn"
          class:active-toggle={settingsStore.showRightPanel}
          aria-pressed={settingsStore.showRightPanel}
          title="Toggle Backlinks panel"
          onclick={() => settingsStore.toggleRightPanel()}
        >
          {@html fluentIcons.backlink16}
          <span>Backlinks</span>
        </button>
      {/if}

      <button
        type="button"
        class="fluent-btn"
        class:active-toggle={settingsStore.showInspector}
        aria-pressed={settingsStore.showInspector}
        title="Toggle live clipboard payload inspector"
        onclick={() => {
          settingsStore.showInspector = !settingsStore.showInspector;
          if (settingsStore.showInspector) onRefreshInspector();
        }}
      >
        {@html fluentIcons.code16}
        <span>Inspector</span>
      </button>

      <div class="settings-wrapper">
        <button
          type="button"
          class="fluent-btn icon-only"
          aria-expanded={settingsStore.showSettings}
          aria-label="Clipboard, editor, and appearance settings"
          title="Settings"
          onclick={() => (settingsStore.showSettings = !settingsStore.showSettings)}
        >
          {@html fluentIcons.settings16}
        </button>

        {#if settingsStore.showSettings}
          <div class="settings-popover" role="dialog" aria-label="Settings">
            <div class="popover-section">
              <div class="popover-heading">Copy Format Mode</div>
              {#each ['markdown_source', 'plain', 'rich'] as const as mode}
                <label class="radio-row">
                  <input
                    type="radio"
                    name="copy-mode-setting"
                    checked={settingsStore.copyMode === mode}
                    onchange={() => handleSelectCopyMode(mode)}
                  />
                  <div class="radio-text">
                    <span class="radio-title">{COPY_MODE_LABELS[mode].label}</span>
                    <span class="radio-desc">{COPY_MODE_LABELS[mode].desc}</span>
                  </div>
                </label>
              {/each}
            </div>

            <div class="popover-section">
              <div class="popover-heading">Editor & Focus Options</div>
              <label class="checkbox-row">
                <input
                  type="checkbox"
                  checked={settingsStore.editingMode === 'live'}
                  onchange={(e) =>
                    settingsStore.setEditingMode(
                      (e.currentTarget as HTMLInputElement).checked
                        ? 'live'
                        : 'source'
                    )}
                />
                <span>Live Preview inline formatting (Ctrl+/)</span>
              </label>
              <label class="checkbox-row">
                <input
                  type="checkbox"
                  checked={settingsStore.typewriterScrolling}
                  onchange={(e) =>
                    settingsStore.setTypewriterScrolling(
                      (e.currentTarget as HTMLInputElement).checked
                    )}
                />
                <span>Typewriter scrolling (center active line)</span>
              </label>
              <label class="checkbox-row">
                <input
                  type="checkbox"
                  checked={settingsStore.dimInactiveParagraphs}
                  onchange={(e) =>
                    settingsStore.setDimInactiveParagraphs(
                      (e.currentTarget as HTMLInputElement).checked
                    )}
                />
                <span>Dim all lines except current paragraph</span>
              </label>
            </div>

            <div class="popover-section">
              <div class="popover-heading">Theme</div>
              <div class="theme-pills">
                {#each ['system', 'light', 'dark'] as const as pref}
                  <button
                    type="button"
                    class="seg-btn"
                    class:active={settingsStore.themePref === pref}
                    onclick={() => settingsStore.setThemePreference(pref)}
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
                  bind:checked={settingsStore.forceSolidBackdrop}
                />
                <span>Use solid-color fallback (Win10 / transparency off)</span>
              </label>
            </div>
          </div>
        {/if}
      </div>
    </div>
  </header>
{/if}

<style>
  .minimal-title-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 28px;
    padding: 0 var(--space-4);
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-muted);
    font-size: var(--text-xs);
    user-select: none;
    flex-shrink: 0;
    opacity: 0.72;
    transition: opacity var(--transition-fast);
  }

  .minimal-title-bar:hover {
    opacity: 1;
  }

  .minimal-left,
  .minimal-right {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .minimal-brand {
    font-weight: 600;
    color: var(--text-secondary);
  }

  .minimal-sep {
    opacity: 0.5;
  }

  .minimal-note {
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .minimal-hint {
    font-size: 11px;
  }

  .minimal-exit-btn {
    border: 1px solid var(--border-subtle);
    background: var(--bg-subtle);
    color: var(--text-secondary);
    border-radius: var(--radius-control);
    font-size: 11px;
    padding: 2px 8px;
    cursor: pointer;
  }

  .minimal-exit-btn:hover {
    background: var(--bg-control-hover);
    color: var(--text-primary);
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
    transition:
      background-color var(--transition-fast),
      opacity var(--transition-fast);
  }

  .command-bar.floating-reveal {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    z-index: 120;
    background: var(--bg-card-solid);
    border-bottom: 1px solid var(--border-control);
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
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
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
    width: 340px;
    background: var(--bg-card-solid);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-card);
    padding: var(--space-4);
    z-index: 150;
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
</style>
