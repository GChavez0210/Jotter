<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import IconButton from './ui/IconButton.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import { PROVIDER_NAMES, type ConflictCopy } from './syncConflicts';
  import { m, trashName } from './i18n.svelte';

  let confirmingPath = $state<string | null>(null);

  let providerName = $derived(
    vaultStore.syncProvider ? PROVIDER_NAMES[vaultStore.syncProvider] : null
  );

  function clientName(copy: ConflictCopy): string {
    if (copy.provider === 'unknown') return m.conflicts_sync_client();
    // Dropbox's naming is shared by Nextcloud and ownCloud; name the folder's client instead.
    if (copy.provider === 'dropbox' && vaultStore.syncProvider && vaultStore.syncProvider !== 'dropbox') {
      return m.conflicts_sync_client();
    }
    return PROVIDER_NAMES[copy.provider];
  }

  function stem(path: string): string {
    return path.split('/').pop()!.replace(/\.md$/i, '');
  }

  async function openBoth(copy: ConflictCopy) {
    if (copy.originalPath) await tabsStore.openNoteInTab(copy.originalPath);
    await tabsStore.openNoteInTab(copy.copyPath);
  }

  async function discard(copy: ConflictCopy) {
    confirmingPath = null;
    await vaultStore.deleteEntryInVault(copy.copyPath);
  }
</script>

<div class="conflicts-panel" role="region" aria-label={m.sidebar_view_conflicts()}>
  <div class="section-header"><span>{m.sidebar_view_conflicts()}</span></div>

  <div class="list-scroll">
    <p class="intro">
      {m.conflicts_intro({ client: providerName ?? m.conflicts_your_client(), trash: trashName() })}
    </p>

    {#if vaultStore.syncConflicts.length === 0}
      <div class="empty-msg">{m.conflicts_none()}</div>
    {:else}
      <ul class="conflict-list">
        {#each vaultStore.syncConflicts as copy (copy.copyPath)}
          <li class="conflict-item">
            <span class="item-icon"><Icon name="warning" /></span>
            <div class="item-text">
              <button
                type="button"
                class="item-name"
                title={m.conflicts_open_path({ path: copy.copyPath })}
                onclick={() => void tabsStore.openNoteInTab(copy.copyPath)}
              >
                {stem(copy.copyPath)}
              </button>
              <div class="item-meta">
                {clientName(copy)} ·
                {#if copy.originalPath}
                  {m.conflicts_copy_of()}
                  <button
                    type="button"
                    class="link"
                    onclick={() => void tabsStore.openNoteInTab(copy.originalPath!)}
                  >{stem(copy.originalPath)}</button>
                {:else}
                  {m.conflicts_original_missing()}
                {/if}
              </div>
              {#if confirmingPath === copy.copyPath}
                <div class="confirm">
                  {m.conflicts_confirm_move({ trash: trashName() })}
                  <button type="button" class="danger" onclick={() => void discard(copy)}>{m.conflicts_move()}</button>
                  <button type="button" onclick={() => (confirmingPath = null)}>{m.common_cancel()}</button>
                </div>
              {/if}
            </div>
            {#if copy.ambiguous}
              <IconButton
                icon="check"
                size="sm"
                label={m.conflicts_dismiss({ name: stem(copy.copyPath) })}
                onclick={() => vaultStore.dismissConflict(copy.copyPath)}
              />
            {/if}
            {#if copy.originalPath}
              <IconButton
                icon="copy-simple"
                size="sm"
                label={m.conflicts_open_both()}
                onclick={() => void openBoth(copy)}
              />
            {/if}
            <IconButton
              icon="trash"
              size="sm"
              label={m.conflicts_move_named({ name: stem(copy.copyPath), trash: trashName() })}
              onclick={() => (confirmingPath = copy.copyPath)}
            />
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .conflicts-panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .section-header {
    padding: var(--space-2) var(--space-2) var(--space-1) var(--space-4);
    color: var(--text-muted);
    font: 600 var(--fs-11) / 1 var(--font-ui);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .list-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 0 var(--space-2) var(--space-2);
  }

  .intro,
  .empty-msg {
    margin: 0;
    padding: var(--space-2);
    color: var(--text-muted);
    font-size: var(--fs-12);
    line-height: 1.45;
  }

  .conflict-list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .conflict-item {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-1) var(--space-1) var(--space-2);
    border-radius: var(--radius-control);
  }

  .conflict-item:hover {
    background: var(--surface-hover);
  }

  .item-icon {
    display: inline-flex;
    flex-shrink: 0;
    padding-top: 3px;
    color: var(--status-warn);
  }

  .item-text {
    flex: 1;
    min-width: 0;
  }

  .item-name {
    display: block;
    max-width: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-primary);
    font: 500 var(--fs-13) / 1.5 var(--font-ui);
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }

  .item-meta {
    color: var(--text-muted);
    font-size: var(--fs-12);
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    cursor: pointer;
  }

  .item-name:hover,
  .link:hover {
    text-decoration: underline;
  }

  .confirm {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    color: var(--text-secondary);
    font-size: var(--fs-12);
  }

  .confirm button {
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--line-control);
    border-radius: var(--radius-control);
    background: var(--surface-app);
    color: var(--text-primary);
    font: inherit;
    cursor: pointer;
  }

  .confirm .danger {
    border-color: var(--status-danger);
    color: var(--status-danger);
  }
</style>
