<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import Button from './ui/Button.svelte';
  import IconButton from './ui/IconButton.svelte';
  import Segmented from './ui/Segmented.svelte';
  import { COPY_MODE_LABELS, settingsStore } from './stores/settings.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import type { CopyMode } from './types';

  interface Props {
    onCopyTrigger: () => void;
    onPastePlainTrigger: () => void;
    onRefreshInspector: () => void;
    onExportHtml: () => void;
    onExportPdf: () => void;
  }

  let {
    onCopyTrigger,
    onPastePlainTrigger,
    onRefreshInspector,
    onExportHtml,
    onExportPdf,
  }: Props = $props();

  const COPY_MODES: CopyMode[] = ['markdown_source', 'plain', 'rich'];

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
        <IconButton
          icon="sidebar-simple"
          label="Toggle sidebar"
          tooltip="Toggle sidebar (Ctrl+\)"
          pressed={!settingsStore.sidebarCollapsed}
          onclick={() => settingsStore.toggleSidebar()}
        />
      {/if}
      <span class="brand-icon"><Icon name="notebook" size={20} /></span>
      <span class="brand-title">Jotter</span>
      {#if vaultStore.vaultSnapshot}
        <span class="note-pill" title={vaultStore.vaultSnapshot.rootPath}>
          {vaultStore.vaultSnapshot.name}
        </span>
      {:else if tabsStore.scratchpadMode}
        <span class="note-pill">Scratchpad</span>
        <Button icon="folder-open" onclick={() => (tabsStore.scratchpadMode = false)}>
          Open or create vault
        </Button>
      {/if}

      {#if settingsStore.focusMode !== 'off'}
        <Button
          pressed
          title="Exit {settingsStore.focusMode} focus mode (Esc)"
          onclick={() => settingsStore.exitFocusMode()}
        >
          Exit {settingsStore.focusMode === 'zen' ? 'Zen' : 'Minimal'} (Esc)
        </Button>
      {/if}
    </div>

    <div class="toolbar-actions">
      <span class="segmented-label">Copy as</span>
      <Segmented
        label="Copy mode"
        value={settingsStore.copyMode}
        options={COPY_MODES.map((mode) => ({
          value: mode,
          label: COPY_MODE_LABELS[mode].label,
          title: COPY_MODE_LABELS[mode].desc,
        }))}
        onchange={handleSelectCopyMode}
      />

      <div class="divider" role="separator"></div>

      <Button variant="primary" icon="copy" title="Copy selection or note (Ctrl+C)" onclick={onCopyTrigger}>Copy</Button>

      <Button icon="clipboard-text" title="Paste clipboard contents strictly as plain text (Ctrl+Shift+V)" onclick={onPastePlainTrigger}>Paste plain</Button>

      <Button icon="export" title="Export active note to self-contained HTML" onclick={onExportHtml}>Export HTML</Button>

      <Button icon="printer" title="Export active note to PDF via Print to PDF (Ctrl+Shift+P)" onclick={onExportPdf}>Print / PDF</Button>

      <Button
        icon="magnifying-glass"
        title="Open Command Palette (Ctrl+P)"
        onclick={() => (settingsStore.showCommandPalette = true)}
      >
        Commands
      </Button>

      {#if vaultStore.vaultSnapshot}
        <Button
          icon="arrow-bend-up-left"
          pressed={settingsStore.showRightPanel}
          title="Toggle Backlinks panel"
          onclick={() => settingsStore.toggleRightPanel()}
        >
          Backlinks
        </Button>
      {/if}

      <Button
        icon="code"
        pressed={settingsStore.showInspector}
        title="Toggle live clipboard payload inspector"
        onclick={() => {
          settingsStore.showInspector = !settingsStore.showInspector;
          if (settingsStore.showInspector) onRefreshInspector();
        }}
      >
        Inspector
      </Button>

      <IconButton
        icon="gear-six"
        label="Settings"
        tooltip="Settings (Ctrl+,)"
        aria-expanded={settingsStore.showSettings}
        onclick={() => (settingsStore.showSettings = !settingsStore.showSettings)}
      />
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
    border-bottom: 1px solid var(--line-subtle);
    color: var(--text-muted);
    font-size: var(--fs-12);
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
    font-size: var(--fs-11);
  }

  .minimal-exit-btn {
    border: 1px solid var(--line-subtle);
    background: var(--surface-subtle);
    color: var(--text-secondary);
    border-radius: var(--radius-control);
    font-size: var(--fs-11);
    padding: 2px 8px;
    cursor: pointer;
  }

  .minimal-exit-btn:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .command-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--line-subtle);
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
    background: var(--surface-raised);
    border-bottom: 1px solid var(--line-control);
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
    color: var(--accent);
  }

  .brand-title {
    font-family: var(--font-display);
    font-size: var(--fs-15);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .note-pill {
    font-size: var(--fs-12);
    color: var(--text-secondary);
    background: var(--surface-subtle);
    border: 1px solid var(--line-subtle);
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

  .segmented-label {
    font-size: var(--fs-12);
    color: var(--text-secondary);
    padding: 0 var(--space-2);
  }

  .divider {
    width: 1px;
    height: 20px;
    background: var(--line-subtle);
    margin: 0 var(--space-1);
  }

</style>

