<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import IconButton from './ui/IconButton.svelte';
  import { listDeletedEntries } from './ipc';
  import { vaultStore } from './stores/vault.svelte';
  import type { DeletedEntry } from './types';

  let entries = $state<DeletedEntry[]>([]);
  let loading = $state(true);
  let errorMessage = $state('');
  let restoringId = $state<string | null>(null);
  let refreshId = 0;

  const dateFormat = new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  });

  async function refresh() {
    const id = ++refreshId;
    loading = true;
    try {
      const result = await listDeletedEntries();
      if (id !== refreshId) return;
      entries = result;
      errorMessage = '';
    } catch (err) {
      if (id !== refreshId) return;
      entries = [];
      errorMessage = err instanceof Error ? err.message : String(err);
    } finally {
      if (id === refreshId) loading = false;
    }
  }

  async function restore(entry: DeletedEntry) {
    restoringId = entry.id;
    const restored = await vaultStore.restoreDeletedEntry(entry.id);
    restoringId = null;
    if (restored) {
      entries = entries.filter((e) => e.id !== entry.id);
    }
  }

  function parentFolder(relPath: string): string {
    const idx = relPath.lastIndexOf('/');
    return idx > 0 ? relPath.slice(0, idx) : 'Vault root';
  }

  // Reload when the vault changes, including notes this window just deleted.
  $effect(() => {
    void vaultStore.vaultSnapshot?.rootPath;
    void vaultStore.deletedVersion;
    void refresh();
  });
</script>

<div class="deleted-panel" aria-label="Recently deleted">
  <div class="section-header">
    <span>Recently deleted</span>
    <IconButton icon="arrow-clockwise" size="sm" label="Refresh" onclick={() => void refresh()} />
  </div>

  <div class="list-scroll">
    {#if loading && entries.length === 0}
      <div class="empty-msg">Reading the Recycle Bin…</div>
    {:else if errorMessage}
      <div class="empty-msg" role="alert">{errorMessage}</div>
    {:else if entries.length === 0}
      <div class="empty-msg">
        No deleted notes from this vault are in the Recycle Bin.
      </div>
    {:else}
      <ul class="deleted-list">
        {#each entries as entry (entry.id)}
          <li class="deleted-item">
            <span class="item-icon">
              <Icon name={entry.isFolder ? 'folder-simple' : 'file-text'} />
            </span>
            <div class="item-text">
              <div class="item-name" title={entry.originalRelPath}>
                {entry.isFolder ? entry.name : entry.name.replace(/\.md$/i, '')}
              </div>
              <div class="item-meta">
                {parentFolder(entry.originalRelPath)} · {dateFormat.format(entry.deletedMs)}
              </div>
            </div>
            <IconButton
              icon="arrow-bend-up-left"
              size="sm"
              label="Restore {entry.originalRelPath}"
              tooltip="Restore to {parentFolder(entry.originalRelPath)}"
              disabled={restoringId !== null}
              onclick={() => void restore(entry)}
            />
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .deleted-panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .section-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-2) var(--space-1) var(--space-4);
    color: var(--text-muted);
    font: 600 var(--fs-11) / 1 var(--font-ui);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .section-header span {
    flex: 1;
  }

  .list-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 0 var(--space-2) var(--space-2);
  }

  .empty-msg {
    padding: var(--space-2) var(--space-2);
    color: var(--text-muted);
    font-size: var(--fs-13);
  }

  .deleted-list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .deleted-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-1) var(--space-1) var(--space-2);
    border-radius: var(--radius-control);
  }

  .deleted-item:hover {
    background: var(--surface-hover);
  }

  .item-icon {
    display: inline-flex;
    flex-shrink: 0;
    color: var(--text-muted);
  }

  .item-text {
    flex: 1;
    min-width: 0;
  }

  .item-name,
  .item-meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-name {
    font-weight: 500;
    color: var(--text-primary);
  }

  .item-meta {
    color: var(--text-muted);
    font-size: var(--fs-12);
  }
</style>
