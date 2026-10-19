<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { settingsStore } from './stores/settings.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import Button from './ui/Button.svelte';
  import Icon from './ui/Icon.svelte';
  import IconButton from './ui/IconButton.svelte';
  import Segmented from './ui/Segmented.svelte';

  interface Props {
    /** True while the sidebar is on screen; the title bar then skips its own sidebar toggle. */
    sidebarVisible: boolean;
  }

  let { sidebarVisible }: Props = $props();

  // macOS keeps its native traffic lights (titleBarStyle Overlay); Windows and Linux get drawn controls.
  const isMac = navigator.userAgent.includes('Mac');
  let maximized = $state(false);

  let hasEditor = $derived(Boolean(tabsStore.activeTab) || tabsStore.scratchpadMode);
  let showTabs = $derived(Boolean(vaultStore.vaultSnapshot) && tabsStore.openTabs.length > 0);

  function createNote() {
    const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
    void vaultStore.createNoteInVault('', `Note ${stamp}`);
  }

  function windowAction(action: 'minimize' | 'toggleMaximize' | 'close') {
    void getCurrentWindow()[action]().catch(() => {
      // Not running inside Tauri (browser preview).
    });
  }

  onMount(() => {
    if (isMac) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    try {
      const win = getCurrentWindow();
      const sync = async () => {
        maximized = await win.isMaximized();
      };
      void sync().catch(() => {});
      void win
        .onResized(() => void sync().catch(() => {}))
        .then((fn) => {
          if (disposed) fn();
          else unlisten = fn;
        })
        .catch(() => {});
    } catch {
      // Not running inside Tauri (browser preview).
    }
    return () => {
      disposed = true;
      unlisten?.();
    };
  });
</script>

<!-- Hover-out hides the revealed bar in focus modes; it is not an interactive control. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<header
  class="titlebar"
  class:mac-inset={isMac && !sidebarVisible}
  class:faded={settingsStore.focusMode === 'minimal' && !settingsStore.topHoverReveal}
  class:floating={settingsStore.focusMode === 'zen'}
  data-tauri-drag-region
  aria-label="Title bar"
  onmouseleave={() => {
    if (settingsStore.focusMode !== 'off' && !settingsStore.showSettings) {
      settingsStore.topHoverReveal = false;
    }
  }}
>
  <div class="lead">
    {#if vaultStore.vaultSnapshot && !sidebarVisible && settingsStore.focusMode === 'off'}
      <IconButton
        icon="sidebar-simple"
        label="Show sidebar"
        tooltip="Show sidebar (Ctrl+\)"
        onclick={() => settingsStore.toggleSidebar()}
      />
    {/if}
    {#if !vaultStore.vaultSnapshot}
      <span class="app-name">
        {tabsStore.scratchpadMode ? 'Scratchpad' : 'Jotter'}
      </span>
      {#if tabsStore.scratchpadMode}
        <Button variant="subtle" icon="folder-open" onclick={() => (tabsStore.scratchpadMode = false)}>
          Open vault
        </Button>
      {/if}
    {/if}
  </div>

  {#if showTabs}
    <div class="tabs" role="tablist" aria-label="Open notes">
      {#each tabsStore.openTabs as tab (tab.relPath)}
        {@const active = tab.relPath === tabsStore.activeTabPath}
        <div
          class="tab"
          class:active
          role="tab"
          tabindex="0"
          aria-selected={active}
          title={tab.relPath}
          onclick={() => (tabsStore.activeTabPath = tab.relPath)}
          onkeydown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              tabsStore.activeTabPath = tab.relPath;
            }
          }}
          onauxclick={(e) => {
            if (e.button === 1) void tabsStore.closeTab(tab.relPath, e);
          }}
        >
          <Icon name="file-text" size={14} />
          <span class="tab-title">{tab.title}</span>
          {#if tab.dirty}
            <span class="dirty-dot" title="Unsaved changes (autosaving…)" aria-label="Unsaved changes"></span>
          {/if}
          <button
            type="button"
            class="tab-close"
            title="Close tab (Ctrl+W)"
            aria-label="Close {tab.title}"
            onclick={(e) => void tabsStore.closeTab(tab.relPath, e)}
          >
            <Icon name="x" size={12} />
          </button>
        </div>
      {/each}
    </div>
  {/if}
  {#if vaultStore.vaultSnapshot}
    <div class="new-tab">
      <IconButton icon="plus" size="sm" label="New note" tooltip="New note (Ctrl+N)" onclick={createNote} />
    </div>
  {/if}

  <div class="drag" data-tauri-drag-region></div>

  <div class="tools">
    {#if settingsStore.focusMode !== 'off'}
      <Button variant="subtle" icon="arrows-in-simple" onclick={() => settingsStore.exitFocusMode()}>
        Exit focus (Esc)
      </Button>
    {/if}
    {#if hasEditor}
      <Segmented
        label="Editor mode"
        value={settingsStore.editingMode}
        options={[
          { value: 'live', label: 'Live', icon: 'eye', title: 'Formatted while you type (Ctrl+/)' },
          { value: 'source', label: 'Source', icon: 'code', title: 'Plain Markdown (Ctrl+/)' },
        ]}
        onchange={(mode) => settingsStore.setEditingMode(mode)}
      />
    {/if}
    {#if vaultStore.vaultSnapshot && settingsStore.focusMode === 'off'}
      <IconButton
        icon="sidebar"
        label="Backlinks panel"
        pressed={settingsStore.showRightPanel}
        onclick={() => settingsStore.toggleRightPanel()}
      />
    {/if}
    <IconButton
      icon="dots-three"
      label="Commands"
      tooltip="Commands (Ctrl+P)"
      onclick={() => (settingsStore.showCommandPalette = true)}
    />
  </div>

  {#if !isMac}
    <div class="window-controls">
      <button type="button" tabindex="-1" aria-label="Minimize" onclick={() => windowAction('minimize')}>
        <Icon name="minus" size={14} />
      </button>
      <button
        type="button"
        tabindex="-1"
        aria-label={maximized ? 'Restore' : 'Maximize'}
        onclick={() => windowAction('toggleMaximize')}
      >
        <Icon name={maximized ? 'copy-simple' : 'square'} size={14} />
      </button>
      <button type="button" tabindex="-1" class="close" aria-label="Close" onclick={() => windowAction('close')}>
        <Icon name="x" size={14} />
      </button>
    </div>
  {/if}
</header>

<style>
  .titlebar {
    height: var(--bar-height);
    flex: none;
    display: flex;
    align-items: stretch;
    gap: var(--space-1);
    border-bottom: 1px solid var(--line-subtle);
    background: var(--surface-editor);
    user-select: none;
    transition: opacity var(--transition-panel);
  }

  .titlebar.mac-inset {
    padding-left: 76px;
  }

  .titlebar.faded {
    opacity: 0;
  }

  .titlebar.faded:hover,
  .titlebar.faded:focus-within {
    opacity: 1;
  }

  .titlebar.floating {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    z-index: 120;
    box-shadow: var(--shadow-popover);
  }

  .lead,
  .tools {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex: none;
  }

  .lead {
    padding-left: var(--space-2);
  }

  .lead:empty {
    padding-left: var(--space-1);
  }

  .app-name {
    padding: 0 var(--space-2);
    font-weight: 600;
    color: var(--text-secondary);
  }

  .tabs {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .tab {
    position: relative;
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    max-width: 200px;
    padding: 0 6px 0 12px;
    border-radius: var(--radius-card) var(--radius-card) 0 0;
    color: var(--text-muted);
    cursor: default;
  }

  .tab:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .tab.active {
    background: var(--surface-editor);
    color: var(--text-primary);
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .tab-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dirty-dot {
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
  }

  .tab-close {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    flex: none;
    padding: 0;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: inherit;
    opacity: 0;
    cursor: pointer;
  }

  .tab:hover .tab-close,
  .tab.active .tab-close,
  .tab-close:focus-visible {
    opacity: 1;
  }

  .tab-close:hover {
    background: var(--surface-active);
  }

  .new-tab {
    display: flex;
    align-items: center;
    flex: none;
  }

  .drag {
    flex: 1;
    min-width: var(--space-4);
  }

  .tools {
    padding-right: var(--space-2);
  }

  .window-controls {
    display: flex;
    flex: none;
  }

  .window-controls button {
    display: grid;
    place-items: center;
    width: 46px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text-secondary);
    cursor: default;
  }

  .window-controls button:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .window-controls button.close:hover {
    background: var(--win-close);
    color: var(--on-win-close);
  }
</style>
