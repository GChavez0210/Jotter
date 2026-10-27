<script lang="ts">
  import { isLinux, isMac, shortcutLabel } from './platform';
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { settingsStore } from './stores/settings.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import Button from './ui/Button.svelte';
  import Icon from './ui/Icon.svelte';
  import IconButton from './ui/IconButton.svelte';
  import Segmented from './ui/Segmented.svelte';
  import { m } from './i18n.svelte';
  import { openInActivePane, paneShowing, toggleSplitNote } from './panes';
  import { noteStamp } from './templates';

  interface Props {
    /** True while the sidebar is on screen; the title bar then skips its own sidebar toggle. */
    sidebarVisible: boolean;
  }

  let { sidebarVisible }: Props = $props();

  // macOS keeps its native traffic lights (titleBarStyle Overlay) and Linux its own title bar;
  // only Windows draws the controls here.
  const drawsWindowControls = !isMac && !isLinux;
  let maximized = $state(false);

  let hasEditor = $derived(Boolean(tabsStore.activeTab) || tabsStore.scratchpadMode);
  let showTabs = $derived(Boolean(vaultStore.vaultSnapshot) && tabsStore.openTabs.length > 0);

  function createNote() {
    const stamp = noteStamp();
    void vaultStore.createNoteInVault('', m.note_default_name({ stamp }));
  }

  async function windowAction(action: 'minimize' | 'toggleMaximize' | 'close') {
    // Native close requests all pass through the backend's save handshake.
    void getCurrentWindow()[action]().catch(() => {
      // Not running inside Tauri (browser preview).
    });
  }

  onMount(() => {
    if (!drawsWindowControls) return;
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
  aria-label={m.titlebar_label()}
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
        label={m.titlebar_show_sidebar()}
        tooltip={m.titlebar_show_sidebar_tooltip({ shortcut: shortcutLabel('Ctrl+\\') })}
        onclick={() => settingsStore.toggleSidebar()}
      />
    {/if}
    {#if !vaultStore.vaultSnapshot}
      {#if !settingsStore.titleBarCompact}
        <span class="app-name">
          {tabsStore.scratchpadMode ? m.scratchpad_title() : 'Jotter'}
        </span>
      {/if}
      {#if tabsStore.scratchpadMode}
        <Button variant="subtle" icon="folder-open" onclick={() => (tabsStore.scratchpadMode = false)}>
          {m.first_run_open_vault()}
        </Button>
      {/if}
    {/if}
  </div>

  {#if showTabs}
    <div class="tabs" role="tablist" aria-multiselectable={settingsStore.splitVisible && settingsStore.splitMode === 'note'} aria-label={m.titlebar_open_notes()}>
      {#each tabsStore.openTabs as tab (tab.relPath)}
        {@const pane = paneShowing(tab.relPath)}
        {@const active = pane !== null}
        <div
          class="tab"
          class:active
          class:in-split={pane === 'secondary'}
          role="tab"
          tabindex="0"
          aria-selected={active}
          aria-keyshortcuts="Delete"
          title={pane === 'secondary' ? m.titlebar_tab_in_split({ path: tab.relPath }) : tab.relPath}
          onclick={() => void openInActivePane(tab.relPath)}
          onkeydown={(e) => {
            if (e.key === 'Delete' || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'w')) {
              e.preventDefault();
              e.stopPropagation();
              void tabsStore.closeTab(tab.relPath);
            } else if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              void openInActivePane(tab.relPath);
            }
          }}
          onauxclick={(e) => {
            if (e.button === 1) void tabsStore.closeTab(tab.relPath, e);
          }}
        >
          <Icon name="file-text" size={14} />
          <span class="tab-title">{tab.title}</span>
          {#if tab.dirty}
            <span class="dirty-dot" title={m.titlebar_unsaved_autosaving()} aria-label={m.titlebar_unsaved()}></span>
          {/if}
          <!-- A button inside the tab would be a widget inside a widget, so this is a plain
               target for the pointer. The keyboard closes the tab with Ctrl+W, and a middle
               click closes it too. -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <span
            class="tab-close"
            aria-hidden="true"
            title={m.titlebar_close_tab({ shortcut: shortcutLabel('Ctrl+W') })}
            onclick={(e) => void tabsStore.closeTab(tab.relPath, e)}
          >
            <Icon name="x" size={12} />
          </span>
        </div>
      {/each}
    </div>
  {/if}
  {#if vaultStore.vaultSnapshot}
    <div class="new-tab">
      <IconButton icon="plus" size="sm" label={m.common_new_note()} tooltip={m.titlebar_new_note_tooltip({ shortcut: shortcutLabel('Ctrl+N') })} onclick={createNote} />
    </div>
  {/if}

  <div class="drag" data-tauri-drag-region></div>

  <div class="tools">
    {#if settingsStore.focusMode !== 'off'}
      <Button variant="subtle" icon="arrows-in-simple" onclick={() => settingsStore.exitFocusMode()}>
        {m.titlebar_exit_focus()}
      </Button>
    {/if}
    {#if hasEditor}
      <Segmented
        label={m.titlebar_editor_mode()}
        compact={settingsStore.titleBarCompact}
        value={settingsStore.editingMode}
        options={[
          { value: 'live', label: m.titlebar_live(), icon: 'eye', title: m.titlebar_live_title({ shortcut: shortcutLabel('Ctrl+/') }) },
          { value: 'source', label: m.editing_mode_source(), icon: 'code', title: m.titlebar_source_title({ shortcut: shortcutLabel('Ctrl+/') }) },
        ]}
        onchange={(mode) => settingsStore.setEditingMode(mode)}
      />
    {/if}
    {#if vaultStore.vaultSnapshot && settingsStore.focusMode === 'off'}
      <IconButton
        icon="columns"
        label={m.split_toggle()}
        tooltip={m.split_toggle_tooltip({ shortcut: shortcutLabel('Ctrl+Shift+\\') })}
        pressed={settingsStore.splitVisible}
        onclick={() => void toggleSplitNote()}
      />
      <IconButton
        icon="sidebar"
        label={m.right_panel()}
        pressed={settingsStore.showRightPanel}
        onclick={() => settingsStore.toggleRightPanel()}
      />
    {/if}
    <IconButton
      icon="dots-three"
      label={m.cmd_list_label()}
      tooltip={m.titlebar_commands_tooltip({ shortcut: shortcutLabel('Ctrl+P') })}
      onclick={() => (settingsStore.showCommandPalette = true)}
    />
  </div>

  {#if drawsWindowControls}
    <div class="window-controls">
      <button type="button" tabindex="-1" aria-label={m.window_minimize()} onclick={() => windowAction('minimize')}>
        <Icon name="minus" size={14} />
      </button>
      <button
        type="button"
        tabindex="-1"
        aria-label={maximized ? m.window_restore() : m.window_maximize()}
        onclick={() => windowAction('toggleMaximize')}
      >
        <Icon name={maximized ? 'copy-simple' : 'square'} size={14} />
      </button>
      <button type="button" tabindex="-1" class="close" aria-label={m.common_close()} onclick={() => windowAction('close')}>
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

  /* The note in the second pane is open too, marked with a dashed underline. */
  .tab.in-split {
    box-shadow: inset 0 -2px 0 var(--line-control);
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
    width: 24px;
    height: 24px;
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
  .tab.active .tab-close {
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
