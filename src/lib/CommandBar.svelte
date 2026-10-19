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

<header class="command-bar" aria-label="Application toolbar">
  <div class="brand-group">
    {#if vaultStore.vaultSnapshot}
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
        aria-label="Clipboard and appearance settings"
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

<style>
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
</style>
