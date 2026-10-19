<script lang="ts">
  import { fluentIcons } from './icons';
  import { getAllTags, getNotesByTag } from './ipc';
  import { settingsStore } from './stores/settings.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import type { CalendarNoteEntry, TagSummary } from './types';

  let tags = $state<TagSummary[]>([]);
  let tagNotes = $state<CalendarNoteEntry[]>([]);
  let filterText = $state('');
  let loadingTags = $state(false);
  let loadingNotes = $state(false);

  let filteredTags = $derived.by(() => {
    const q = filterText.trim().replace(/^#/, '').toLowerCase();
    if (!q) return tags;
    return tags.filter((t) => t.tag.toLowerCase().includes(q));
  });

  $effect(() => {
    const version = vaultStore.indexVersion;
    void version;

    if (!vaultStore.vaultSnapshot) {
      tags = [];
      tagNotes = [];
      return;
    }

    loadingTags = true;
    void getAllTags()
      .then((res) => {
        tags = res;
        if (
          settingsStore.selectedTag &&
          !res.some((t) => t.tag === settingsStore.selectedTag)
        ) {
          settingsStore.selectedTag = null;
        }
      })
      .catch(() => {
        tags = [];
      })
      .finally(() => {
        loadingTags = false;
      });
  });

  $effect(() => {
    const activeTag = settingsStore.selectedTag;
    const version = vaultStore.indexVersion;
    void version;

    if (!activeTag || !vaultStore.vaultSnapshot) {
      tagNotes = [];
      loadingNotes = false;
      return;
    }

    loadingNotes = true;
    void getNotesByTag(activeTag)
      .then((res) => {
        if (settingsStore.selectedTag === activeTag) {
          tagNotes = res;
        }
      })
      .catch(() => {
        if (settingsStore.selectedTag === activeTag) {
          tagNotes = [];
        }
      })
      .finally(() => {
        if (settingsStore.selectedTag === activeTag) {
          loadingNotes = false;
        }
      });
  });

  function selectTag(tag: string) {
    settingsStore.selectedTag =
      settingsStore.selectedTag === tag ? null : tag;
  }
</script>

<div class="tag-browser" aria-label="Tag browser">
  <div class="tag-filter-bar">
    <div class="filter-input-wrap">
      <span class="filter-icon">{@html fluentIcons.tag16}</span>
      <input
        type="search"
        class="filter-input"
        placeholder="Filter tags…"
        aria-label="Filter tags"
        bind:value={filterText}
      />
    </div>
  </div>

  <div class="tags-section">
    <div class="section-header">
      <span>Tags ({filteredTags.length})</span>
      {#if settingsStore.selectedTag}
        <button
          type="button"
          class="clear-tag-btn"
          onclick={() => (settingsStore.selectedTag = null)}
        >
          Clear filter
        </button>
      {/if}
    </div>

    {#if loadingTags && tags.length === 0}
      <div class="empty-msg">Loading tags…</div>
    {:else if filteredTags.length === 0}
      <div class="empty-msg">
        {tags.length === 0
          ? 'No #tags or YAML frontmatter tags found in this vault yet.'
          : 'No tags match your filter.'}
      </div>
    {:else}
      <div class="tag-pills" role="listbox" aria-label="Vault tags">
        {#each filteredTags as item (item.tag)}
          <button
            type="button"
            role="option"
            aria-selected={settingsStore.selectedTag === item.tag}
            class="tag-pill"
            class:active={settingsStore.selectedTag === item.tag}
            onclick={() => selectTag(item.tag)}
          >
            <span class="tag-hash">#</span>
            <span class="tag-name">{item.tag}</span>
            <span class="tag-count">{item.count}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>

  {#if settingsStore.selectedTag}
    <div class="filtered-notes-section">
      <div class="section-header">
        <span>Notes tagged <strong>#{settingsStore.selectedTag}</strong> ({tagNotes.length})</span>
      </div>

      <div class="notes-scroll">
        {#if loadingNotes && tagNotes.length === 0}
          <div class="empty-msg">Loading notes…</div>
        {:else if tagNotes.length === 0}
          <div class="empty-msg">No notes found with this tag.</div>
        {:else}
          <ul class="tag-note-list" aria-label="Notes with selected tag">
            {#each tagNotes as note (note.path)}
              <li>
                <button
                  type="button"
                  class="tag-note-item"
                  class:active={tabsStore.activeTabPath === note.path}
                  onclick={() => void tabsStore.openNoteInTab(note.path)}
                >
                  <div class="note-row-top">
                    <span class="note-icon">{@html fluentIcons.document16}</span>
                    <span class="note-title">{note.title}</span>
                  </div>
                  {#if note.path !== `${note.title}.md`}
                    <div class="note-path">{note.path}</div>
                  {/if}
                  {#if note.preview}
                    <div class="note-preview">{note.preview}</div>
                  {/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .tag-browser {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .tag-filter-bar {
    padding: var(--space-2);
    border-bottom: 1px solid var(--border-subtle);
  }

  .filter-input-wrap {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-min-height);
    padding: 0 var(--space-2);
    border-radius: var(--radius-control);
    border: 1px solid var(--border-control);
    background: var(--bg-card-solid);
  }

  .filter-input-wrap:focus-within {
    border-color: var(--accent-color, var(--accent-fallback));
  }

  .filter-icon {
    display: inline-flex;
    color: var(--text-secondary);
  }

  .filter-input {
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    color: var(--text-primary);
    font-family: var(--font-ui);
    font-size: var(--text-xs);
    outline: none;
  }

  .tags-section {
    padding: var(--space-2);
    overflow-y: auto;
    max-height: 52%;
    border-bottom: 1px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .clear-tag-btn {
    border: none;
    background: transparent;
    color: var(--accent-color, var(--accent-fallback));
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    text-transform: none;
  }

  .clear-tag-btn:hover {
    text-decoration: underline;
  }

  .tag-pills {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  .tag-pill {
    min-height: 28px;
    padding: 2px 8px;
    border-radius: var(--radius-control);
    border: 1px solid var(--border-subtle);
    background: var(--bg-subtle);
    color: var(--text-primary);
    font-size: var(--text-xs);
    display: inline-flex;
    align-items: center;
    gap: 4px;
    cursor: pointer;
    transition:
      background-color var(--transition-fast),
      border-color var(--transition-fast);
  }

  .tag-pill:hover {
    background: var(--bg-control-hover);
    border-color: var(--border-control);
  }

  .tag-pill.active {
    background: var(--selection-bg);
    border-color: var(--accent-color, var(--accent-fallback));
    font-weight: 600;
  }

  .tag-hash {
    color: var(--accent-color, var(--accent-fallback));
    font-weight: 600;
  }

  .tag-count {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-card-solid);
    padding: 0 5px;
    border-radius: 999px;
  }

  .filtered-notes-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: var(--space-2);
    gap: var(--space-2);
    overflow: hidden;
  }

  .notes-scroll {
    flex: 1;
    overflow-y: auto;
  }

  .tag-note-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .tag-note-item {
    width: 100%;
    text-align: left;
    padding: var(--space-2);
    border-radius: var(--radius-control);
    border: 1px solid var(--border-subtle);
    background: var(--bg-subtle);
    color: var(--text-primary);
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .tag-note-item:hover {
    background: var(--bg-control-hover);
    border-color: var(--border-control);
  }

  .tag-note-item.active {
    background: var(--selection-bg);
    border-color: var(--accent-color, var(--accent-fallback));
  }

  .note-row-top {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .note-icon {
    display: inline-flex;
    color: var(--accent-color, var(--accent-fallback));
    flex-shrink: 0;
  }

  .note-title {
    font-size: var(--text-xs);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .note-path {
    font-size: 11px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .note-preview {
    font-size: 11px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty-msg {
    padding: var(--space-3) var(--space-2);
    text-align: center;
    font-size: var(--text-xs);
    color: var(--text-secondary);
  }
</style>
