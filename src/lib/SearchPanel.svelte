<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import { searchVault } from './ipc';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import type { SearchHit } from './types';

  let query = $state('');
  let results = $state<SearchHit[]>([]);
  let searching = $state(false);
  let searchInputEl: HTMLInputElement | undefined = $state();

  interface SnippetSegment {
    text: string;
    highlighted: boolean;
  }

  function decodeHtmlEntities(input: string): string {
    return input
      .replace(/&amp;/g, '&')
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>')
      .replace(/&quot;/g, '"')
      .replace(/&#39;/g, "'");
  }

  function parseHighlightedSnippet(snippetHtml: string): SnippetSegment[] {
    const segments: SnippetSegment[] = [];
    const regex = /<mark>([\s\S]*?)<\/mark>/gi;
    let lastIndex = 0;
    let match: RegExpExecArray | null;

    while ((match = regex.exec(snippetHtml)) !== null) {
      if (match.index > lastIndex) {
        segments.push({
          text: decodeHtmlEntities(snippetHtml.slice(lastIndex, match.index)),
          highlighted: false,
        });
      }
      segments.push({
        text: decodeHtmlEntities(match[1]),
        highlighted: true,
      });
      lastIndex = regex.lastIndex;
    }

    if (lastIndex < snippetHtml.length) {
      segments.push({
        text: decodeHtmlEntities(snippetHtml.slice(lastIndex)),
        highlighted: false,
      });
    }

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
    const timer = setTimeout(() => {
      void searchVault(q, 50)
        .then((hits) => {
          if (query.trim() === q) {
            results = hits;
          }
        })
        .catch(() => {
          if (query.trim() === q) {
            results = [];
          }
        })
        .finally(() => {
          if (query.trim() === q) {
            searching = false;
          }
        });
    }, 90);

    return () => clearTimeout(timer);
  });

  export function focusSearchInput() {
    searchInputEl?.focus();
    searchInputEl?.select();
  }

  function handleSelectHit(hit: SearchHit) {
    void tabsStore.openNoteInTab(hit.path, {
      lineNumber: hit.lineNumber,
    });
  }
</script>

<div class="search-panel" aria-label="Full-text vault search">
  <div class="search-box-wrap">
    <div class="search-input-container">
      <span class="search-icon"><Icon name="magnifying-glass" /></span>
      <input
        bind:this={searchInputEl}
        type="search"
        class="search-input"
        placeholder="Search inside notes"
        aria-label="Search vault notes"
        bind:value={query}
      />
      {#if query}
        <button
          type="button"
          class="clear-btn"
          aria-label="Clear search query"
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
          Searching…
        {:else}
          {results.length} {results.length === 1 ? 'note' : 'notes'} matched
        {/if}
      </div>
    {/if}
  </div>

  <div class="results-scroll">
    {#if !query.trim()}
      <div class="empty-hint">
        Search the text of every note in this vault.
      </div>
    {:else if !searching && results.length === 0}
      <div class="empty-hint">
        No notes matched <strong>"{query.trim()}"</strong>.
      </div>
    {:else}
      <ul class="results-list" aria-label="Search results">
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
                <span class="hit-meta" title="First match on line {hit.lineNumber}">
                  {hit.matchCount}
                </span>
              </div>
              {#if hit.path !== `${hit.title}.md`}
                <div class="hit-path">{hit.path}</div>
              {/if}
              <div class="hit-snippet">
                {#each parseHighlightedSnippet(hit.snippetHtml) as seg, i (i)}
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
