<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import Dialog from './ui/Dialog.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { stripMdExt, vaultStore } from './stores/vault.svelte';
  import type { VaultNode } from './types';
  import { validateEntryName } from './validation';

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  let query = $state('');
  let selectedIndex = $state(0);
  let inputEl: HTMLInputElement | undefined = $state();

  interface FuzzyNoteMatch {
    node: VaultNode;
    title: string;
    relPathNoExt: string;
    score: number;
  }

  /**
   * Scores `query` against `candidate` using subsequence fuzzy matching.
   * Returns `null` if `query` is not a subsequence of `candidate`.
   */
  function scoreFuzzy(queryLower: string, candidate: string): number | null {
    if (!queryLower) return 1;
    const candLower = candidate.toLowerCase();

    // Fast exact/substring boost
    if (candLower === queryLower) return 1000;
    if (candLower.startsWith(queryLower)) return 600 - candidate.length;
    const subIdx = candLower.indexOf(queryLower);
    if (subIdx >= 0) {
      return 400 - subIdx * 2 - candidate.length * 0.2;
    }

    // Subsequence match
    let qIdx = 0;
    let score = 0;
    let consecutive = 0;

    for (let i = 0; i < candLower.length && qIdx < queryLower.length; i++) {
      if (candLower[i] === queryLower[qIdx]) {
        score += 12;
        if (consecutive > 0) {
          score += consecutive * 8;
        }
        if (
          i === 0 ||
          candLower[i - 1] === '/' ||
          candLower[i - 1] === ' ' ||
          candLower[i - 1] === '-' ||
          candLower[i - 1] === '_'
        ) {
          score += 15;
        }
        consecutive++;
        qIdx++;
      } else {
        consecutive = 0;
        score -= 1;
      }
    }

    return qIdx === queryLower.length ? score : null;
  }

  let matchedNotes = $derived.by<FuzzyNoteMatch[]>(() => {
    const q = query.trim().toLowerCase();
    const files = vaultStore.allFiles;
    const list: FuzzyNoteMatch[] = [];

    for (const node of files) {
      const title = stripMdExt(node.name);
      const relPathNoExt = stripMdExt(node.relPath);
      if (!q) {
        const isOpen = tabsStore.openTabs.some((t) => t.relPath === node.relPath);
        list.push({
          node,
          title,
          relPathNoExt,
          score: isOpen ? 50 : 0,
        });
        continue;
      }

      const titleScore = scoreFuzzy(q, title);
      const pathScore = scoreFuzzy(q, relPathNoExt);
      const best =
        titleScore !== null && pathScore !== null
          ? Math.max(titleScore + 20, pathScore)
          : titleScore !== null
            ? titleScore + 20
            : pathScore;

      if (best !== null) {
        list.push({
          node,
          title,
          relPathNoExt,
          score: best,
        });
      }
    }

    list.sort(
      (a, b) => b.score - a.score || a.title.localeCompare(b.title)
    );
    return list.slice(0, 40);
  });

  let canCreateFromQuery = $derived.by(() => {
    const trimmed = query.trim();
    if (!trimmed || !vaultStore.vaultSnapshot) return false;
    const leafName = trimmed.split('/').pop() ?? trimmed;
    if (validateEntryName(leafName) !== null) return false;
    return !vaultStore.resolveWikiTarget(trimmed);
  });

  $effect(() => {
    void query;
    selectedIndex = 0;
  });

  $effect(() => {
    inputEl?.focus();
  });

  function openMatch(item: FuzzyNoteMatch) {
    onClose();
    void tabsStore.openNoteInTab(item.node.relPath);
  }

  async function createFromQuery() {
    const trimmed = query.trim().replace(/\\/g, '/');
    if (!trimmed) return;
    const slashIdx = trimmed.lastIndexOf('/');
    const parentFolder = slashIdx >= 0 ? trimmed.slice(0, slashIdx) : '';
    const noteName = slashIdx >= 0 ? trimmed.slice(slashIdx + 1) : trimmed;
    onClose();
    await vaultStore.createNoteInVault(parentFolder, noteName);
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (matchedNotes.length > 0) {
        selectedIndex = (selectedIndex + 1) % matchedNotes.length;
      }
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (matchedNotes.length > 0) {
        selectedIndex =
          (selectedIndex - 1 + matchedNotes.length) % matchedNotes.length;
      }
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      const chosen = matchedNotes[selectedIndex];
      if (chosen && !e.shiftKey) {
        openMatch(chosen);
      } else if (canCreateFromQuery) {
        void createFromQuery();
      }
    }
  }

  let listEl: HTMLDivElement | undefined = $state();

  // Keep the keyboard selection visible while arrowing through a long list.
  $effect(() => {
    void selectedIndex;
    listEl?.querySelector('.picker-row.selected')?.scrollIntoView({ block: 'nearest' });
  });
</script>

<Dialog label="Quick switcher" {onClose} onkeydown={handleKeyDown}>
  <div class="picker-search">
    <Icon name="magnifying-glass" />
    <input
      bind:this={inputEl}
      type="text"
      class="picker-input"
      placeholder="Find a note by name"
      aria-label="Find a note by name"
      bind:value={query}
    />
    <kbd>Esc</kbd>
  </div>

  <div class="picker-list" bind:this={listEl} role="listbox" aria-label="Matching notes">
    {#if matchedNotes.length === 0 && !canCreateFromQuery}
      <div class="picker-empty">No notes in this vault match “{query.trim()}”.</div>
    {:else}
      {#each matchedNotes as item, idx (item.node.relPath)}
        <button
          type="button"
          role="option"
          aria-selected={idx === selectedIndex}
          class="picker-row"
          class:selected={idx === selectedIndex}
          onmouseenter={() => (selectedIndex = idx)}
          onclick={() => openMatch(item)}
        >
          <Icon name="file-text" />
          <span class="picker-label">{item.title}</span>
          {#if item.relPathNoExt !== item.title}
            <span class="picker-hint">{item.relPathNoExt.split('/').slice(0, -1).join(' / ')}</span>
          {/if}
        </button>
      {/each}

      {#if canCreateFromQuery}
        <button
          type="button"
          class="picker-row"
          class:selected={matchedNotes.length === 0}
          onclick={() => void createFromQuery()}
        >
          <Icon name="file-plus" />
          <span class="picker-label">Create “{query.trim()}”</span>
          <kbd>{matchedNotes.length === 0 ? 'Enter' : 'Shift+Enter'}</kbd>
        </button>
      {/if}
    {/if}
  </div>

  <div class="picker-footer">
    <span><kbd>↑</kbd> <kbd>↓</kbd> to move</span>
    <span><kbd>Enter</kbd> to open</span>
    <span><kbd>Shift+Enter</kbd> to create</span>
  </div>
</Dialog>
