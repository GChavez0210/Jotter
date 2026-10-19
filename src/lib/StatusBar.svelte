<script lang="ts">
  import { clipboardStore } from './stores/clipboard.svelte';
  import { settingsStore } from './stores/settings.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import Icon from './ui/Icon.svelte';

  // Vault › folders › note, built from the active tab's vault-relative path.
  let crumbs = $derived.by(() => {
    const parts: string[] = [];
    if (vaultStore.vaultSnapshot) parts.push(vaultStore.vaultSnapshot.name);
    if (tabsStore.activeTab) {
      const folders = tabsStore.activeTab.relPath.replace(/\\/g, '/').split('/').slice(0, -1);
      parts.push(...folders, tabsStore.activeTab.title.replace(/\.md$/i, ''));
    } else if (tabsStore.scratchpadMode) {
      parts.push('Scratchpad');
    }
    return parts;
  });
</script>

{#if settingsStore.focusMode === 'off'}
  <footer class="status-bar" aria-label="Status bar">
    <nav class="crumbs" aria-label="Location">
      {#each crumbs as crumb, i}
        {#if i > 0}<Icon name="caret-right" size={10} />{/if}
        <span class:current={i === crumbs.length - 1}>{crumb}</span>
      {/each}
    </nav>

    <span class="message" role="status" title={clipboardStore.statusBanner}>
      {clipboardStore.statusBanner}
    </span>

    {#if tabsStore.activeTab || tabsStore.scratchpadMode}
      <span class="item">{settingsStore.editingMode === 'live' ? 'Live' : 'Source'}</span>
      <span class="item">
        {#if tabsStore.docStats.selectedChars > 0}
          {tabsStore.docStats.selectedChars} selected ·
        {/if}
        {tabsStore.docStats.words} words
      </span>
    {/if}
  </footer>
{/if}

<style>
  .status-bar {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    height: 26px;
    flex-shrink: 0;
    padding: 0 var(--space-3);
    border-top: 1px solid var(--line-subtle);
    color: var(--text-muted);
    font-size: var(--fs-12);
    font-variant-numeric: tabular-nums;
    user-select: none;
  }

  .crumbs {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
    flex-shrink: 1;
    overflow: hidden;
    white-space: nowrap;
  }

  .crumbs span {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .crumbs .current {
    color: var(--text-secondary);
    font-weight: 500;
  }

  .message {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: right;
  }

  .item {
    flex: none;
    white-space: nowrap;
  }
</style>
