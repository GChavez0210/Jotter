<script lang="ts">
  import { fluentIcons } from './icons';
  import { getBacklinks } from './ipc';
  import { settingsStore } from './stores/settings.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import type { BacklinkEntry } from './types';

  let backlinks = $state<BacklinkEntry[]>([]);
  let loading = $state(false);

  $effect(() => {
    const activeRel = tabsStore.activeTabPath;
    const version = vaultStore.indexVersion;
    void version;

    if (!activeRel || !vaultStore.vaultSnapshot) {
      backlinks = [];
      loading = false;
      return;
    }

    loading = true;
    void getBacklinks(activeRel)
      .then((res) => {
        if (tabsStore.activeTabPath === activeRel) {
          backlinks = res;
        }
      })
      .catch(() => {
        if (tabsStore.activeTabPath === activeRel) {
          backlinks = [];
        }
      })
      .finally(() => {
        if (tabsStore.activeTabPath === activeRel) {
          loading = false;
        }
      });
  });

  function handleOpenBacklink(entry: BacklinkEntry) {
    void tabsStore.openNoteInTab(entry.sourcePath, {
      lineNumber: entry.lineNumber,
    });
  }
</script>

{#if settingsStore.showRightPanel}
  <aside class="right-panel" aria-label="Backlinks panel">
    <div class="panel-header">
      <div class="panel-title-wrap">
        <span class="panel-icon">{@html fluentIcons.backlink16}</span>
        <span class="panel-title">Backlinks</span>
        <span class="count-badge">{backlinks.length}</span>
      </div>
      <button
        type="button"
        class="fluent-icon-btn"
        title="Close Backlinks panel"
        aria-label="Close Backlinks panel"
        onclick={() => settingsStore.toggleRightPanel()}
      >
        {@html fluentIcons.dismiss16}
      </button>
    </div>

    <div class="panel-body">
      {#if !tabsStore.activeTab}
        <div class="empty-state">
          Open a note to see other vault notes that link to it.
        </div>
      {:else if loading && backlinks.length === 0}
        <div class="empty-state">Loading backlinks…</div>
      {:else if backlinks.length === 0}
        <div class="empty-state">
          <div>No backlinks to <strong>{tabsStore.activeTab.title}</strong> yet.</div>
          <div class="empty-hint">
            Link to this note from another file using <code>[[{tabsStore.activeTab.title}]]</code>.
          </div>
        </div>
      {:else}
        <ul class="backlink-list" aria-label="Incoming links to active note">
          {#each backlinks as item (`${item.sourcePath}:${item.lineNumber}:${item.target}`)}
            <li>
              <button
                type="button"
                class="backlink-card"
                title="Open {item.sourcePath} at line {item.lineNumber}"
                onclick={() => handleOpenBacklink(item)}
              >
                <div class="backlink-top">
                  <span class="source-icon">{@html fluentIcons.document16}</span>
                  <span class="source-title">{item.sourceTitle}</span>
                  <span class="line-pill">L{item.lineNumber}</span>
                </div>
                {#if item.sourcePath !== `${item.sourceTitle}.md`}
                  <div class="source-path">{item.sourcePath}</div>
                {/if}
                <div class="line-context">{item.lineContext}</div>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </aside>
{/if}

<style>
  .right-panel {
    width: 268px;
    height: 100%;
    flex-shrink: 0;
    background: var(--surface-editor);
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-card);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    user-select: none;
    transition: background-color var(--transition-fast);
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--line-subtle);
  }

  .panel-title-wrap {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .panel-icon {
    display: inline-flex;
    color: var(--accent);
  }

  .panel-title {
    font-size: var(--fs-12);
    font-weight: 600;
    color: var(--text-primary);
  }

  .count-badge {
    font-family: var(--font-mono);
    font-size: var(--fs-11);
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--surface-subtle);
    border: 1px solid var(--line-subtle);
    color: var(--text-secondary);
  }

  .fluent-icon-btn {
    min-width: var(--control-min-height);
    min-height: var(--control-min-height);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-control);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      color var(--transition-fast);
  }

  .fluent-icon-btn:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .panel-body {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-2);
  }

  .empty-state {
    padding: var(--space-4) var(--space-3);
    text-align: center;
    color: var(--text-secondary);
    font-size: var(--fs-12);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .empty-hint {
    color: var(--text-muted);
    font-size: var(--fs-11);
  }

  .empty-hint code {
    font-family: var(--font-mono);
    background: var(--surface-code);
    padding: 1px 4px;
    border-radius: var(--radius-control);
  }

  .backlink-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .backlink-card {
    width: 100%;
    text-align: left;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    border: 1px solid var(--line-subtle);
    background: var(--surface-subtle);
    color: var(--text-primary);
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 4px;
    transition:
      background-color var(--transition-fast),
      border-color var(--transition-fast);
  }

  .backlink-card:hover {
    background: var(--surface-hover);
    border-color: var(--line-control);
  }

  .backlink-top {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
  }

  .source-icon {
    display: inline-flex;
    color: var(--accent);
    flex-shrink: 0;
  }

  .source-title {
    font-size: var(--fs-12);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .line-pill {
    font-family: var(--font-mono);
    font-size: var(--fs-11);
    color: var(--text-muted);
    background: var(--surface-raised);
    border: 1px solid var(--line-subtle);
    padding: 0 5px;
    border-radius: var(--radius-control);
    flex-shrink: 0;
  }

  .source-path {
    font-size: var(--fs-11);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .line-context {
    font-family: var(--font-mono);
    font-size: var(--fs-11);
    color: var(--text-secondary);
    background: var(--surface-code);
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-control);
    padding: 4px 6px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
