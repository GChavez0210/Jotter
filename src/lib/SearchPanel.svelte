<script lang="ts">
  import { fluentIcons } from './icons';
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
      <span class="search-icon">{@html fluentIcons.search16}</span>
      <input
        bind:this={searchInputEl}
        type="search"
        class="search-input"
        placeholder="Search notes (tantivy full-text)…"
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
          {@html fluentIcons.dismiss16}
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
        Type keywords or phrases above to search across all Markdown notes in the vault.
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
                <span class="hit-icon">{@html fluentIcons.document16}</span>
                <span class="hit-title">{hit.title}</span>
                <span class="hit-meta">
                  L{hit.lineNumber} · {hit.matchCount}
                  {hit.matchCount === 1 ? 'hit' : 'hits'}
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
    padding: var(--space-2);
    border-bottom: 1px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .search-input-container {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-min-height);
    padding: 0 var(--space-2);
    border-radius: var(--radius-control);
    border: 1px solid var(--border-control);
    background: var(--bg-card-solid);
  }

  .search-input-container:focus-within {
    border-color: var(--accent-color, var(--accent-fallback));
  }

  .search-icon {
    display: inline-flex;
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .search-input {
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    color: var(--text-primary);
    font-family: var(--font-ui);
    font-size: var(--text-xs);
    outline: none;
  }

  .clear-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .clear-btn:hover {
    background: var(--bg-control-hover);
    color: var(--text-primary);
  }

  .search-summary {
    font-size: 11px;
    color: var(--text-secondary);
    padding: 0 2px;
  }

  .results-scroll {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-2);
  }

  .empty-hint {
    padding: var(--space-4) var(--space-2);
    text-align: center;
    font-size: var(--text-xs);
    color: var(--text-secondary);
  }

  .results-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .hit-card {
    width: 100%;
    text-align: left;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    border: 1px solid var(--border-subtle);
    background: var(--bg-subtle);
    color: var(--text-primary);
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 4px;
    transition:
      background-color var(--transition-fast),
      border-color var(--transition-fast);
  }

  .hit-card:hover {
    background: var(--bg-control-hover);
    border-color: var(--border-control);
  }

  .hit-header {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
  }

  .hit-icon {
    display: inline-flex;
    color: var(--accent-color, var(--accent-fallback));
    flex-shrink: 0;
  }

  .hit-title {
    font-size: var(--text-xs);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .hit-meta {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-card-solid);
    border: 1px solid var(--border-subtle);
    padding: 1px 5px;
    border-radius: var(--radius-control);
    flex-shrink: 0;
  }

  .hit-path {
    font-size: 11px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hit-snippet {
    font-size: 11px;
    line-height: 1.45;
    color: var(--text-secondary);
    background: var(--bg-card-solid);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-control);
    padding: 4px 6px;
    word-break: break-word;
  }

  .snippet-mark {
    background: var(--selection-bg);
    color: var(--text-primary);
    font-weight: 600;
    padding: 0 2px;
    border-radius: 2px;
  }
</style>
