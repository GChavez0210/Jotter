<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import { tagColor } from './tagColor';
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
      <span class="filter-icon"><Icon name="tag" /></span>
      <input
        type="search"
        class="filter-input"
        placeholder="Filter tags"
        aria-label="Filter tags"
        bind:value={filterText}
      />
    </div>
  </div>

  <div class="tags-section">
    <div class="section-header">
      <span>Tags</span>
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
      <div class="tag-list" role="listbox" aria-label="Vault tags">
        {#each filteredTags as item (item.tag)}
          <button
            type="button"
            role="option"
            aria-selected={settingsStore.selectedTag === item.tag}
            class="tag-row"
            class:active={settingsStore.selectedTag === item.tag}
            onclick={() => selectTag(item.tag)}
          >
            <span class="tag-dot" style:background={tagColor(item.tag)}></span>
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
        <span>#{settingsStore.selectedTag} · {tagNotes.length} {tagNotes.length === 1 ? 'note' : 'notes'}</span>
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
                    <span class="note-icon"><Icon name="file-text" /></span>
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
    padding: var(--space-1) var(--space-3) var(--space-2);
  }

  .filter-input-wrap {
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

  .filter-input-wrap:focus-within {
    border-color: var(--accent);
  }

  .filter-icon {
    display: inline-flex;
  }

  .filter-input {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    color: var(--text-primary);
    font: inherit;
  }

  /* The surrounding box shows focus with its accent border. */
  .filter-input:focus-visible {
    outline: none;
  }

  .filter-input::-webkit-search-cancel-button {
    display: none;
  }

  .tags-section {
    display: flex;
    flex-direction: column;
    max-height: 50%;
    min-height: 0;
  }

  .section-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-2) var(--space-1) var(--space-4);
    color: var(--text-muted);
    font: 600 var(--fs-11) / 1 var(--font-ui);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .section-header span {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .clear-tag-btn {
    padding: 2px 6px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--accent);
    font: 500 var(--fs-12) var(--font-ui);
    letter-spacing: 0;
    text-transform: none;
    cursor: pointer;
  }

  .clear-tag-btn:hover {
    background: var(--surface-hover);
  }

  .tag-list {
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    padding: 0 var(--space-2) var(--space-2);
  }

  .tag-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--row-height);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .tag-row:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .tag-row.active {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 500;
  }

  .tag-dot {
    width: 8px;
    height: 8px;
    flex: none;
    margin: 0 4px;
    border-radius: 50%;
  }

  .tag-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag-count {
    color: var(--text-muted);
    font-size: var(--fs-12);
    font-variant-numeric: tabular-nums;
  }

  .filtered-notes-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--line-subtle);
  }

  .notes-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 0 var(--space-2) var(--space-2);
  }

  .empty-msg {
    padding: var(--space-2) var(--space-4);
    color: var(--text-muted);
    font-size: var(--fs-13);
  }

  .tag-note-list {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tag-note-item {
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

  .tag-note-item:hover {
    background: var(--surface-hover);
  }

  .tag-note-item.active {
    background: var(--accent-soft);
  }

  .note-row-top {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .note-icon {
    display: inline-flex;
    flex-shrink: 0;
    color: var(--text-muted);
  }

  .note-title {
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag-note-item.active .note-title {
    color: var(--accent);
  }

  .note-path,
  .note-preview {
    overflow: hidden;
    padding-left: 24px;
    color: var(--text-muted);
    font-size: var(--fs-12);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .note-preview {
    color: var(--text-secondary);
  }
</style>
