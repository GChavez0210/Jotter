<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import { searchVault } from './ipc';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import type { SearchHit } from './types';
  import { m, SLOT, splitSlots } from './i18n.svelte';

  let query = $state('');
  const noMatchParts = $derived(splitSlots(m.search_no_match({ query: SLOT })));
  let results = $state<SearchHit[]>([]);
  let searching = $state(false);
  let searchInputEl: HTMLInputElement | undefined = $state();
  let resultsEl: HTMLUListElement | undefined = $state();
  let searchSeq = 0;

  interface SnippetSegment {
    text: string;
    highlighted: boolean;
  }

  /**
   * Splits a snippet at the match ranges the backend sent. The ranges count UTF-16 code units,
   * which is what `String.slice` indexes by, so they apply to the text as it is.
   */
  function snippetSegments(hit: SearchHit): SnippetSegment[] {
    const segments: SnippetSegment[] = [];
    let at = 0;
    for (const { start, end } of hit.matches) {
      if (start < at || end > hit.snippet.length || end <= start) continue;
      if (start > at) segments.push({ text: hit.snippet.slice(at, start), highlighted: false });
      segments.push({ text: hit.snippet.slice(start, end), highlighted: true });
      at = end;
    }
    if (at < hit.snippet.length) segments.push({ text: hit.snippet.slice(at), highlighted: false });
    return segments;
  }

  $effect(() => {
    const q = query.trim();
    const version = vaultStore.indexVersion;
    void version;

    if (!q) {
      results = [];
      searching = false;
      return;
    }

    searching = true;
    // Only the newest request may update the list. The backend also cancels an older search
    // that is still running when a newer one starts.
    const mine = ++searchSeq;
    const timer = setTimeout(() => {
      void searchVault(q, 50)
        .then((hits) => {
          if (mine === searchSeq) results = hits;
        })
        .catch(() => {
          if (mine === searchSeq) results = [];
        })
        .finally(() => {
          if (mine === searchSeq) searching = false;
        });
    }, 90);

    return () => clearTimeout(timer);
  });

  export function focusSearchInput() {
    searchInputEl?.focus();
    searchInputEl?.select();
  }

  function hitButtons(): HTMLButtonElement[] {
    return Array.from(resultsEl?.querySelectorAll<HTMLButtonElement>('.hit-card') ?? []);
  }

  /** Arrow keys move from the box into the results and between them; Up from the first returns. */
  function onInputKeydown(e: KeyboardEvent) {
    if (e.isComposing) return;
    if (e.key === 'ArrowDown') {
      const first = hitButtons()[0];
      if (first) {
        e.preventDefault();
        first.focus();
      }
    } else if (e.key === 'Enter' && results[0]) {
      e.preventDefault();
      handleSelectHit(results[0]);
    }
  }

  function onResultsKeydown(e: KeyboardEvent) {
    const buttons = hitButtons();
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (at === -1) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      buttons[Math.min(at + 1, buttons.length - 1)].focus();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (at === 0) searchInputEl?.focus();
      else buttons[at - 1].focus();
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      buttons[e.key === 'Home' ? 0 : buttons.length - 1].focus();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      searchInputEl?.focus();
    }
  }

  function handleSelectHit(hit: SearchHit) {
    void tabsStore.openNoteInTab(hit.path, {
      lineNumber: hit.lineNumber,
    });
  }
</script>

<div class="search-panel" aria-label={m.search_panel()}>
  <div class="search-box-wrap">
    <div class="search-input-container">
      <span class="search-icon"><Icon name="magnifying-glass" /></span>
      <input
        bind:this={searchInputEl}
        type="search"
        class="search-input"
        placeholder={m.search_placeholder()}
        aria-label={m.search_label()}
        bind:value={query}
        onkeydown={onInputKeydown}
      />
      {#if query}
        <button
          type="button"
          class="clear-btn"
          aria-label={m.search_clear()}
          onclick={() => {
            query = '';
            searchInputEl?.focus();
          }}
        >
          <Icon name="x" />
        </button>
      {/if}
    </div>
    {#if query.trim()}
      <div class="search-summary">
        {#if searching}
          {m.search_searching()}
        {:else}
          {m.search_count({ count: results.length })}
        {/if}
      </div>
    {/if}
  </div>

  <div class="results-scroll">
    {#if !query.trim()}
      <div class="empty-hint">
        {m.search_empty_hint()}
        <span class="syntax-hint">{m.search_syntax_hint()}</span>
      </div>
    {:else if !searching && results.length === 0}
      <div class="empty-hint">
        {noMatchParts[0]}<strong>"{query.trim()}"</strong>{noMatchParts[1]}
      </div>
    {:else}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <ul class="results-list" aria-label={m.search_results()} bind:this={resultsEl} onkeydown={onResultsKeydown}>
        {#each results as hit (hit.path)}
          <li>
            <button
              type="button"
              class="hit-card"
              onclick={() => handleSelectHit(hit)}
            >
              <div class="hit-header">
                <span class="hit-icon"><Icon name="file-text" /></span>
                <span class="hit-title">{hit.title}</span>
                <span class="hit-meta" title={m.search_first_match({ line: hit.lineNumber })}>
                  {hit.matchCount}
                </span>
              </div>
              {#if hit.path !== `${hit.title}.md`}
                <div class="hit-path">{hit.path}</div>
              {/if}
              <div class="hit-snippet">
                {#each snippetSegments(hit) as seg, i (i)}
                  {#if seg.highlighted}
                    <mark class="snippet-mark">{seg.text}</mark>
                  {:else}
                    <span>{seg.text}</span>
                  {/if}
                {/each}
              </div>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .search-panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .search-box-wrap {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-3) var(--space-2);
  }

  .search-input-container {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 30px;
    padding: 0 var(--space-2);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-card);
    background: var(--surface-editor);
    color: var(--text-muted);
  }

  .search-input-container:focus-within {
    border-color: var(--accent);
  }

  .search-icon {
    display: inline-flex;
    flex-shrink: 0;
  }

  .search-input {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    color: var(--text-primary);
    font: inherit;
  }

  /* The surrounding box shows focus with its accent border. */
  .search-input:focus-visible {
    outline: none;
  }

  .search-input::-webkit-search-cancel-button {
    display: none;
  }

  .clear-btn {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }

  .clear-btn:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .search-summary {
    padding: 0 2px;
    color: var(--text-muted);
    font-size: var(--fs-12);
  }

  .results-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 0 var(--space-2) var(--space-2);
  }

  .empty-hint {
    padding: var(--space-3) var(--space-2);
    color: var(--text-muted);
    font-size: var(--fs-13);
  }

  .results-list {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .hit-card {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: var(--space-2);
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .hit-card:hover {
    background: var(--surface-hover);
  }

  .hit-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .hit-icon {
    display: inline-flex;
    flex-shrink: 0;
    color: var(--text-muted);
  }

  .hit-title {
    flex: 1;
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hit-meta {
    flex-shrink: 0;
    color: var(--text-muted);
    font-size: var(--fs-12);
    font-variant-numeric: tabular-nums;
  }

  .hit-path {
    overflow: hidden;
    padding-left: 24px;
    color: var(--text-muted);
    font-size: var(--fs-12);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .syntax-hint {
    display: block;
    margin-top: var(--space-2);
    font-size: var(--fs-12);
  }

  .hit-snippet {
    display: -webkit-box;
    overflow: hidden;
    padding-left: 24px;
    color: var(--text-secondary);
    font-size: var(--fs-12);
    line-height: 1.5;
    word-break: break-word;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
  }

  .snippet-mark {
    padding: 0 1px;
    border-radius: 2px;
    background: var(--mark);
    color: inherit;
  }
</style>
