<script lang="ts">
  import { fluentIcons } from './icons';
  import { tabsStore } from './stores/tabs.svelte';
  import { stripMdExt, vaultStore } from './stores/vault.svelte';
  import type { VaultNode } from './types';
  import { validateWindowsEntryName } from './validation';

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
    if (validateWindowsEntryName(leafName) !== null) return false;
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
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="switcher-backdrop"
  onmousedown={(e) => {
    if (e.target === e.currentTarget) onClose();
  }}
  onkeydown={handleKeyDown}
>
  <div
    class="switcher-dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Quick switcher"
  >
    <div class="search-row">
      <span class="search-icon">{@html fluentIcons.search16}</span>
      <input
        bind:this={inputEl}
        type="text"
        class="search-input"
        placeholder="Quick switcher — fuzzy match note title or path (Ctrl+O)…"
        aria-label="Fuzzy search note names"
        bind:value={query}
      />
      <kbd class="esc-hint">Esc</kbd>
    </div>

    <div class="note-list" role="listbox" aria-label="Matching vault notes">
      {#if matchedNotes.length === 0 && !canCreateFromQuery}
        <div class="empty-results">No matching notes in this vault.</div>
      {:else}
        {#each matchedNotes as item, idx (item.node.relPath)}
          <button
            type="button"
            role="option"
            aria-selected={idx === selectedIndex}
            class="note-row"
            class:selected={idx === selectedIndex}
            onmouseenter={() => (selectedIndex = idx)}
            onclick={() => openMatch(item)}
          >
            <div class="note-left">
              <span class="note-icon">{@html fluentIcons.document16}</span>
              <span class="note-title">{item.title}</span>
              {#if item.relPathNoExt !== item.title}
                <span class="note-path">{item.relPathNoExt}</span>
              {/if}
            </div>
            <kbd class="enter-hint">Enter</kbd>
          </button>
        {/each}

        {#if canCreateFromQuery}
          <button
            type="button"
            class="note-row create-row"
            class:selected={matchedNotes.length === 0}
            onclick={() => void createFromQuery()}
          >
            <div class="note-left">
              <span class="note-icon">{@html fluentIcons.documentAdd16}</span>
              <span class="note-title">
                Create new note <strong>"{query.trim()}"</strong>
              </span>
            </div>
            <kbd class="enter-hint">
              {matchedNotes.length === 0 ? 'Enter' : 'Shift+Enter'}
            </kbd>
          </button>
        {/if}
      {/if}
    </div>
  </div>
</div>

<style>
  .switcher-backdrop {
    position: fixed;
    inset: 0;
    z-index: 205;
    background: rgba(0, 0, 0, 0.32);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 10vh;
  }

  .switcher-dialog {
    width: min(540px, 92vw);
    max-height: 68vh;
    background: var(--surface-raised);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-card);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .search-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3);
    border-bottom: 1px solid var(--line-subtle);
  }

  .search-icon {
    display: inline-flex;
    color: var(--text-secondary);
  }

  .search-input {
    flex: 1;
    border: none;
    background: transparent;
    color: var(--text-primary);
    font-family: var(--font-ui);
    font-size: var(--fs-13);
    outline: none;
  }

  .esc-hint,
  .enter-hint {
    font-family: var(--font-mono);
    font-size: var(--fs-11);
    padding: 2px 6px;
    border-radius: var(--radius-control);
    border: 1px solid var(--line-subtle);
    background: var(--surface-subtle);
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .note-list {
    overflow-y: auto;
    padding: var(--space-2);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .empty-results {
    padding: var(--space-4);
    text-align: center;
    font-size: var(--fs-12);
    color: var(--text-secondary);
  }

  .note-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    width: 100%;
    min-height: 36px;
    padding: var(--space-1) var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-primary);
    text-align: left;
    cursor: pointer;
    transition: background-color var(--transition-fast);
  }

  .note-row:hover,
  .note-row.selected {
    background: var(--selection);
    border-color: var(--accent);
  }

  .note-left {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .note-icon {
    display: inline-flex;
    color: var(--accent);
    flex-shrink: 0;
  }

  .note-title {
    font-size: var(--fs-12);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .note-path {
    font-size: var(--fs-11);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .create-row {
    margin-top: 2px;
    border-top: 1px dashed var(--line-subtle);
  }
</style>
