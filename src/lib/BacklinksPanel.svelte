<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import { getBacklinks } from './ipc';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import type { BacklinkEntry } from './types';
  import { m, SLOT, splitSlots } from './i18n.svelte';

  let backlinks = $state<BacklinkEntry[]>([]);
  let loading = $state(false);
  const noBacklinksParts = $derived(splitSlots(m.backlinks_none({ title: SLOT })));
  const linkHintParts = $derived(splitSlots(m.backlinks_hint({ link: SLOT })));

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

<section class="panel-section" aria-label={m.backlinks_title()}>
  <div class="panel-header">
    <div class="panel-title-wrap">
      <span class="panel-icon"><Icon name="arrow-bend-up-left" /></span>
      <span class="panel-title">{m.backlinks_title()}</span>
      <span class="count-badge">{backlinks.length}</span>
    </div>
  </div>

  <div class="panel-body">
    {#if !tabsStore.activeTab}
      <div class="empty-state">
        {m.backlinks_no_note()}
      </div>
    {:else if loading && backlinks.length === 0}
      <div class="empty-state">{m.backlinks_loading()}</div>
    {:else if backlinks.length === 0}
      <div class="empty-state">
        <div>{noBacklinksParts[0]}<strong>{tabsStore.activeTab.title}</strong>{noBacklinksParts[1]}</div>
        <div class="empty-hint">
          {linkHintParts[0]}<code>[[{tabsStore.activeTab.title}]]</code>{linkHintParts[1]}
        </div>
      </div>
    {:else}
      <ul class="backlink-list" aria-label={m.backlinks_list()}>
        {#each backlinks as item (`${item.sourcePath}:${item.lineNumber}:${item.target}`)}
          <li>
            <button
              type="button"
              class="backlink-card"
              title={m.backlinks_open_at({ path: item.sourcePath, line: item.lineNumber })}
              onclick={() => handleOpenBacklink(item)}
            >
              <div class="backlink-top">
                <span class="source-icon"><Icon name="file-text" /></span>
                <span class="source-title">{item.sourceTitle}</span>
                <span class="line-pill">{m.backlinks_line_pill({ line: item.lineNumber })}</span>
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
</section>

<style>
  .panel-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
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
