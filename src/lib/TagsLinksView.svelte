<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import { buildTagTree, type TagNode } from './noteGraph';
  import { noteCardsStore } from './stores/noteCards.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import type { NoteCard } from './types';

  interface Props {
    activeRelPath: string | null;
    onSelectFile: (relPath: string) => void;
  }

  let { activeRelPath, onSelectFile }: Props = $props();

  // Rows are keyed by their position in the tree, since a note can appear under several tags.
  let expanded = $state<Record<string, boolean>>({});

  $effect(() => {
    void noteCardsStore.refresh(vaultStore.indexVersion);
  });

  let tree = $derived(buildTagTree(noteCardsStore.cards));
  let byPath = $derived(new Map(noteCardsStore.cards.map((c) => [c.path, c])));

  function toggle(key: string) {
    expanded = { ...expanded, [key]: !expanded[key] };
  }

  // Link chains stop at notes already on the path, so cycles cannot expand forever.
  function linkedCards(card: NoteCard, chain: string[]): NoteCard[] {
    return card.linksTo
      .filter((p) => !chain.includes(p))
      .map((p) => byPath.get(p))
      .filter((c): c is NoteCard => c !== undefined)
      .sort((a, b) => a.title.localeCompare(b.title));
  }
</script>

{#snippet noteRow(card: NoteCard, depth: number, chain: string[], key: string)}
  {@const links = linkedCards(card, chain)}
  <li>
    <div class="row" class:active={card.path === activeRelPath} style:padding-left="{depth * 14 + 4}px">
      {#if links.length}
        <button type="button" class="chevron" aria-label="Links from {card.title}" aria-expanded={!!expanded[key]} onclick={() => toggle(key)}>
          <Icon name={expanded[key] ? 'caret-down' : 'caret-right'} size={12} />
        </button>
      {:else}
        <span class="chevron"></span>
      {/if}
      <button type="button" class="label" title={card.path} aria-current={card.path === activeRelPath ? 'true' : undefined} onclick={() => onSelectFile(card.path)}>
        <Icon name={depth > 0 && chain.length > 1 ? 'link-simple' : 'file-text'} size={14} />
        <span>{card.title}</span>
      </button>
      {#if card.linkedFrom > 0}<span class="count" title="Notes linking here">{card.linkedFrom}</span>{/if}
    </div>
    {#if links.length && expanded[key]}
      <ul>
        {#each links as linked (linked.path)}
          {@render noteRow(linked, depth + 1, [...chain, linked.path], `${key}>${linked.path}`)}
        {/each}
      </ul>
    {/if}
  </li>
{/snippet}

{#snippet tagRow(node: TagNode, depth: number)}
  {@const key = `#${node.tag}`}
  <li>
    <button type="button" class="row tag-row" style:padding-left="{depth * 14 + 4}px" aria-expanded={!!expanded[key]} onclick={() => toggle(key)}>
      <span class="chevron"><Icon name={expanded[key] ? 'caret-down' : 'caret-right'} size={12} /></span>
      <span class="label static"><Icon name="tag" size={14} /><span>{node.name}</span></span>
      <span class="count">{node.total}</span>
    </button>
    {#if expanded[key]}
      <ul>
        {#each node.children as child (child.tag)}
          {@render tagRow(child, depth + 1)}
        {/each}
        {#each node.notes as card (card.path)}
          {@render noteRow(card, depth + 1, [card.path], `${key}/${card.path}`)}
        {/each}
      </ul>
    {/if}
  </li>
{/snippet}

<div class="tags-links" role="region" aria-label="Notes by tag and link">
  {#if noteCardsStore.error}
    <div class="empty" role="alert">{noteCardsStore.error}</div>
  {:else if noteCardsStore.cards.length === 0}
    <div class="empty">{noteCardsStore.loading ? 'Loading notes…' : 'This vault has no notes yet.'}</div>
  {:else}
    <ul class="tree">
      {#each tree.roots as node (node.tag)}
        {@render tagRow(node, 0)}
      {/each}
      {#if tree.untagged.length}
        <li>
          <button type="button" class="row tag-row" aria-expanded={!!expanded['untagged']} onclick={() => toggle('untagged')}>
            <span class="chevron"><Icon name={expanded['untagged'] ? 'caret-down' : 'caret-right'} size={12} /></span>
            <span class="label static muted"><Icon name="file-text" size={14} /><span>No tags</span></span>
            <span class="count">{tree.untagged.length}</span>
          </button>
          {#if expanded['untagged']}
            <ul>
              {#each tree.untagged as card (card.path)}
                {@render noteRow(card, 1, [card.path], `untagged/${card.path}`)}
              {/each}
            </ul>
          {/if}
        </li>
      {/if}
    </ul>
  {/if}
</div>

<style>
  .tags-links {
    padding: 0 var(--space-2) var(--space-2);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 2px;
    width: 100%;
    min-height: var(--control-min-height);
    padding-right: var(--space-2);
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    font-size: var(--fs-12);
    text-align: left;
  }

  .row:hover {
    background: var(--surface-hover);
  }

  .row.active {
    background: var(--selection);
    font-weight: 600;
  }

  .tag-row {
    cursor: pointer;
  }

  .chevron {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 16px;
    height: 16px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .label {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .label :global(svg) {
    flex: none;
    color: var(--text-secondary);
  }

  .label span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .muted {
    color: var(--text-secondary);
  }

  .count {
    flex: none;
    color: var(--text-muted);
    font-size: var(--fs-11);
    font-variant-numeric: tabular-nums;
  }

  .empty {
    padding: var(--space-2);
    color: var(--text-muted);
    font-size: var(--fs-13);
  }
</style>
