<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './ui/Icon.svelte';
  import IconButton from './ui/IconButton.svelte';
  import { getVaultHealth } from './ipc';
  import { settingsStore } from './stores/settings.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { stripMdExt, vaultStore } from './stores/vault.svelte';
  import type { BrokenLink, VaultHealthReport } from './types';
  import { validateEntryName } from './validation';
  import { m, trashName } from './i18n.svelte';

  let report = $state<VaultHealthReport | null>(null);
  let scanning = $state(false);
  let error = $state('');
  let confirmingPath = $state<string | null>(null);

  let problemCount = $derived(
    report
      ? report.brokenLinks.total +
          report.unusedAttachments.total +
          report.emptyNotes.total +
          report.duplicateTitles.total
      : 0
  );

  async function scan() {
    if (scanning) return;
    scanning = true;
    error = '';
    confirmingPath = null;
    const root = vaultStore.vaultSnapshot?.rootPath;
    try {
      const result = await getVaultHealth(settingsStore.templatesFolder);
      // A vault switch while scanning must not show the old vault's report.
      if (vaultStore.vaultSnapshot?.rootPath === root) report = result;
    } catch (err) {
      error = m.health_failed({ error: err instanceof Error ? err.message : String(err) });
    } finally {
      scanning = false;
    }
  }

  onMount(() => void scan());

  function name(path: string): string {
    return stripMdExt(path.split('/').pop() ?? path);
  }

  function folder(path: string): string {
    return path.split('/').slice(0, -1).join(' / ');
  }

  /** Splits a link target such as `Folder/Note` into the folder and note name to create. */
  function createParts(target: string): { parent: string; leaf: string } | null {
    const clean = target.trim().replace(/\\/g, '/');
    const slash = clean.lastIndexOf('/');
    const leaf = slash >= 0 ? clean.slice(slash + 1) : clean;
    if (!leaf || validateEntryName(leaf) !== null) return null;
    return { parent: slash >= 0 ? clean.slice(0, slash) : '', leaf };
  }

  async function createMissing(link: BrokenLink) {
    const parts = createParts(link.target);
    if (!parts) return;
    await vaultStore.createNoteInVault(parts.parent, parts.leaf);
    await scan();
  }

  async function moveToTrash(path: string) {
    confirmingPath = null;
    await vaultStore.deleteEntryInVault(path);
    await scan();
  }
</script>

{#snippet confirmRow(path: string)}
  {#if confirmingPath === path}
    <div class="confirm">
      {m.health_confirm_move({ trash: trashName() })}
      <button type="button" class="danger" onclick={() => void moveToTrash(path)}>{m.conflicts_move()}</button>
      <button type="button" onclick={() => (confirmingPath = null)}>{m.common_cancel()}</button>
    </div>
  {/if}
{/snippet}

{#snippet more(total: number, shown: number)}
  {#if total > shown}
    <li class="more">{m.health_more({ count: total - shown })}</li>
  {/if}
{/snippet}

<div class="health-panel" role="region" aria-label={m.sidebar_view_health()}>
  <div class="section-header">
    <span>{m.sidebar_view_health()}</span>
    <IconButton
      icon="arrow-clockwise"
      size="sm"
      label={scanning ? m.health_scanning() : m.health_scan()}
      onclick={() => void scan()}
    />
  </div>

  <div class="list-scroll">
    <p class="intro">{m.health_intro()}</p>

    {#if error}
      <p class="notice">{error}</p>
    {:else if !report}
      <p class="notice">{m.health_scanning()}</p>
    {:else if problemCount === 0}
      <p class="notice">{m.health_all_clear()}</p>
    {:else}
      {#if report.brokenLinks.total > 0}
        <details open>
          <summary>{m.health_broken()} <span class="count">{report.brokenLinks.total}</span></summary>
          <ul class="items">
            <!-- One line can hold the same broken link twice, so the index keeps keys unique. -->
            {#each report.brokenLinks.items as link, idx (`${idx}:${link.sourcePath}:${link.target}`)}
              <li class="item">
                <span class="item-icon"><Icon name="warning" /></span>
                <div class="item-text">
                  <button
                    type="button"
                    class="item-name"
                    title={link.lineContext}
                    onclick={() => void tabsStore.openNoteInTab(link.sourcePath)}
                  >
                    {link.kind === 'wiki' ? `[[${link.target}]]` : `(${link.target})`}
                  </button>
                  <div class="item-meta">
                    {name(link.sourcePath)} · {m.health_line({ line: link.lineNumber })}
                  </div>
                </div>
                {#if link.kind === 'wiki' && createParts(link.target)}
                  <IconButton
                    icon="file-plus"
                    size="sm"
                    label={m.health_create_note({ name: link.target })}
                    onclick={() => void createMissing(link)}
                  />
                {/if}
              </li>
            {/each}
            {@render more(report.brokenLinks.total, report.brokenLinks.items.length)}
          </ul>
        </details>
      {/if}

      {#if report.unusedAttachments.total > 0}
        <details open>
          <summary>{m.health_unused()} <span class="count">{report.unusedAttachments.total}</span></summary>
          <ul class="items">
            {#each report.unusedAttachments.items as path (path)}
              <li class="item">
                <span class="item-icon"><Icon name="file-text" /></span>
                <div class="item-text">
                  <div class="item-label" title={path}>{path.split('/').pop()}</div>
                  {#if folder(path)}<div class="item-meta">{folder(path)}</div>{/if}
                  {@render confirmRow(path)}
                </div>
                <IconButton
                  icon="trash"
                  size="sm"
                  label={m.health_move_named({ name: path, trash: trashName() })}
                  onclick={() => (confirmingPath = path)}
                />
              </li>
            {/each}
            {@render more(report.unusedAttachments.total, report.unusedAttachments.items.length)}
          </ul>
        </details>
      {/if}

      {#if report.emptyNotes.total > 0}
        <details open>
          <summary>{m.health_empty()} <span class="count">{report.emptyNotes.total}</span></summary>
          <ul class="items">
            {#each report.emptyNotes.items as path (path)}
              <li class="item">
                <span class="item-icon"><Icon name="file-text" /></span>
                <div class="item-text">
                  <button
                    type="button"
                    class="item-name"
                    title={path}
                    onclick={() => void tabsStore.openNoteInTab(path)}
                  >
                    {name(path)}
                  </button>
                  {#if folder(path)}<div class="item-meta">{folder(path)}</div>{/if}
                  {@render confirmRow(path)}
                </div>
                <IconButton
                  icon="trash"
                  size="sm"
                  label={m.health_move_named({ name: name(path), trash: trashName() })}
                  onclick={() => (confirmingPath = path)}
                />
              </li>
            {/each}
            {@render more(report.emptyNotes.total, report.emptyNotes.items.length)}
          </ul>
        </details>
      {/if}

      {#if report.duplicateTitles.total > 0}
        <details open>
          <summary>{m.health_duplicates()} <span class="count">{report.duplicateTitles.total}</span></summary>
          <ul class="items">
            {#each report.duplicateTitles.items as group (group.title.toLowerCase())}
              <li class="item">
                <span class="item-icon"><Icon name="copy-simple" /></span>
                <div class="item-text">
                  <div class="item-label">{group.title}</div>
                  {#each group.paths as path (path)}
                    <button
                      type="button"
                      class="link"
                      title={path}
                      onclick={() => void tabsStore.openNoteInTab(path)}
                    >
                      {path}
                    </button>
                  {/each}
                </div>
              </li>
            {/each}
            {@render more(report.duplicateTitles.total, report.duplicateTitles.items.length)}
          </ul>
        </details>
      {/if}
    {/if}
  </div>
</div>

<style>
  .health-panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
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
  .notice {
    margin: 0;
    padding: var(--space-2);
    color: var(--text-muted);
    font-size: var(--fs-12);
    line-height: 1.45;
  }

  summary {
    padding: var(--space-2) var(--space-2) var(--space-1);
    color: var(--text-secondary);
    font: 600 var(--fs-12) / 1.4 var(--font-ui);
    cursor: pointer;
  }

  .count {
    margin-left: var(--space-1);
    color: var(--text-muted);
    font-weight: 500;
  }

  .items {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .item {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-1) var(--space-1) var(--space-2);
    border-radius: var(--radius-control);
  }

  .item:hover {
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

  .item-name,
  .link {
    display: block;
    max-width: 100%;
    padding: 0;
    border: 0;
    background: none;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }

  .item-name {
    color: var(--text-primary);
    font: 500 var(--fs-13) / 1.5 var(--font-ui);
  }

  .link {
    color: var(--accent);
    font: var(--fs-12) / 1.5 var(--font-ui);
  }

  .item-label {
    color: var(--text-primary);
    font: 500 var(--fs-13) / 1.5 var(--font-ui);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-meta {
    color: var(--text-muted);
    font-size: var(--fs-12);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-name:hover,
  .link:hover {
    text-decoration: underline;
  }

  .more {
    padding: var(--space-1) var(--space-2);
    color: var(--text-muted);
    font-size: var(--fs-12);
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
