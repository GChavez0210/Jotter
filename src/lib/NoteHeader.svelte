<script lang="ts">
  import { settingsStore } from './stores/settings.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import Icon from './ui/Icon.svelte';
  import { tagColor } from './tagColor';

  // Same rule as the indexer (links.rs): '#' at line start or after whitespace,
  // then letters, digits, '_', '-' or '/', with at least one non-digit.
  const INLINE_TAG_RE = /(?<=^|\s)#([\p{L}\p{N}_/-]+)/gu;

  let tab = $derived(tabsStore.activeTab);
  let title = $derived(tab ? tab.title.replace(/\.md$/i, '') : 'Scratchpad');
  let folder = $derived(
    tab ? tab.relPath.replace(/\\/g, '/').split('/').slice(0, -1).join(' / ') : ''
  );

  let modifiedMs = $derived(
    tab
      ? vaultStore.allFiles.find((f) => f.relPath.toLowerCase() === tab.relPath.toLowerCase())
          ?.modifiedMs
      : undefined
  );

  let edited = $derived.by(() => {
    if (!modifiedMs) return '';
    const date = new Date(modifiedMs);
    const today = new Date();
    const yesterday = new Date(today);
    yesterday.setDate(today.getDate() - 1);
    if (date.toDateString() === today.toDateString()) {
      return `Edited ${date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })}`;
    }
    if (date.toDateString() === yesterday.toDateString()) return 'Edited yesterday';
    return `Edited ${date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })}`;
  });

  let tagText = $state('');
  $effect(() => {
    const text = tab?.content ?? tabsStore.scratchpadContent;
    const timer = setTimeout(() => { tagText = text; }, 150);
    return () => clearTimeout(timer);
  });
  let tags = $derived.by(() => {
    const text = tagText;
    const found = new Set<string>();
    let inFence = false;
    for (const line of text.split('\n')) {
      if (/^\s*(```|~~~)/.test(line)) {
        inFence = !inFence;
        continue;
      }
      if (inFence) continue;
      const withoutCode = line.replace(/`[^`]*`/g, '');
      for (const match of withoutCode.matchAll(INLINE_TAG_RE)) {
        if (!/^\d+$/.test(match[1])) found.add(match[1].toLowerCase());
      }
    }
    return [...found];
  });

  function showTag(tag: string) {
    settingsStore.selectedTag = tag;
    settingsStore.setLeftSidebarView('tags');
    if (settingsStore.sidebarCollapsed) settingsStore.toggleSidebar();
  }
</script>

<h1 class="title">{title}</h1>
<div class="meta">
  {#if folder}
    <span class="meta-item"><Icon name="folder-simple" size={13} />{folder}</span>
  {/if}
  {#if edited}
    <span class="meta-item">{edited}</span>
  {/if}
  <span class="meta-item">{tabsStore.docStats.words} words</span>
  {#each tags as tag (tag)}
    <button type="button" class="chip" title="Show notes tagged #{tag}" onclick={() => showTag(tag)}>
      <span class="dot" style:background={tagColor(tag)}></span>{tag}
    </button>
  {/each}
</div>

<style>
  .title {
    margin: 0 0 var(--space-3);
    font: 600 var(--fs-32) / 1.2 var(--font-display);
    letter-spacing: -0.01em;
    text-wrap: balance;
    color: var(--text-primary);
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 14px;
    color: var(--text-muted);
    font-size: var(--fs-12);
  }

  .meta-item {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 22px;
    padding: 0 var(--space-2);
    border: 1px solid var(--line-subtle);
    border-radius: 11px;
    background: var(--surface-app);
    color: var(--text-secondary);
    font: inherit;
    cursor: pointer;
  }

  .chip:hover {
    border-color: var(--line-control);
    color: var(--text-primary);
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }
</style>
