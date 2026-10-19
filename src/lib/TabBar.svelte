<script lang="ts">
  import { fluentIcons } from './icons';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';

  function handleQuickCreateNote() {
    const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
    void vaultStore.createNoteInVault('', `Note ${stamp}`);
  }
</script>

<div class="tab-bar" role="tablist" aria-label="Open notes">
  <div class="tabs-scroll">
    {#each tabsStore.openTabs as tab (tab.relPath)}
      <div
        class="tab-item"
        class:active={tab.relPath === tabsStore.activeTabPath}
        role="tab"
        tabindex="0"
        aria-selected={tab.relPath === tabsStore.activeTabPath}
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
        <span class="tab-icon">{@html fluentIcons.document16}</span>
        <span class="tab-title">{tab.title}</span>
        {#if tab.dirty}
          <span
            class="dirty-dot"
            title="Unsaved changes (autosaving…)"
            aria-label="Unsaved changes"
          ></span>
        {/if}
        <button
          type="button"
          class="tab-close-btn"
          title="Close tab (Ctrl+W)"
          aria-label="Close {tab.title}"
          onclick={(e) => void tabsStore.closeTab(tab.relPath, e)}
        >
          {@html fluentIcons.dismiss16}
        </button>
      </div>
    {/each}
  </div>

  <button
    type="button"
    class="new-tab-btn"
    title="New note (Ctrl+N)"
    aria-label="New note"
    onclick={handleQuickCreateNote}
  >
    {@html fluentIcons.add20}
  </button>
</div>

<style>
  .tab-bar {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-height: 36px;
    flex-shrink: 0;
  }

  .tabs-scroll {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    overflow-x: auto;
    scrollbar-width: none;
  }

  .tab-item {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-min-height);
    max-width: 220px;
    padding: 0 var(--space-2) 0 var(--space-3);
    border-radius: var(--radius-control);
    border: 1px solid var(--border-subtle);
    background: var(--bg-subtle);
    color: var(--text-secondary);
    font-size: var(--text-xs);
    cursor: pointer;
    user-select: none;
    transition:
      background-color var(--transition-fast),
      color var(--transition-fast),
      border-color var(--transition-fast);
  }

  .tab-item:hover {
    background: var(--bg-control-hover);
    color: var(--text-primary);
  }

  .tab-item.active {
    background: var(--bg-card-solid);
    color: var(--text-primary);
    border-color: var(--border-control);
    font-weight: 600;
  }

  .tab-icon {
    display: inline-flex;
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .tab-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dirty-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent-color, var(--accent-fallback));
    flex-shrink: 0;
  }

  .tab-close-btn {
    width: 22px;
    height: 22px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    padding: 0;
  }

  .tab-close-btn:hover {
    background: var(--bg-control-hover);
    color: var(--text-primary);
  }

  .new-tab-btn {
    min-width: var(--control-min-height);
    min-height: var(--control-min-height);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-control);
    border: 1px solid var(--border-subtle);
    background: var(--bg-subtle);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background-color var(--transition-fast);
  }

  .new-tab-btn:hover {
    background: var(--bg-control-hover);
    color: var(--text-primary);
  }
</style>
