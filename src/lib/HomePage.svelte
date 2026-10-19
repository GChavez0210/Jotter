<script lang="ts">
  import Button from './ui/Button.svelte';
  import Icon from './ui/Icon.svelte';
  import IconButton from './ui/IconButton.svelte';
  import {
    buildHomeSections,
    DEFAULT_HOME_SECTIONS,
    SECTION_KIND_LABELS,
    type HomeSection,
    type HomeSectionKind,
    type HomeSort,
  } from './noteGraph';
  import { noteCardsStore } from './stores/noteCards.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import { tagColor } from './tagColor';
  import type { NoteCard } from './types';

  interface Props {
    onNewNote: () => void;
  }

  let { onNewNote }: Props = $props();

  const STORAGE_KEY = 'jotter.homeSections';

  function loadAll(): Record<string, HomeSection[]> {
    try {
      const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}');
      return saved && typeof saved === 'object' ? saved : {};
    } catch {
      return {};
    }
  }

  // Sections are per vault, because folder and tag filters name things inside one vault.
  let allSections = $state<Record<string, HomeSection[]>>(loadAll());
  let root = $derived(vaultStore.vaultSnapshot?.rootPath ?? '');
  let sections = $derived(allSections[root] ?? DEFAULT_HOME_SECTIONS);
  let editing = $state(false);

  $effect(() => {
    void noteCardsStore.refresh(vaultStore.indexVersion);
  });

  let filled = $derived(
    buildHomeSections(sections, noteCardsStore.cards, {
      openedPaths: noteCardsStore.openedPaths,
      now: new Date(),
    })
  );
  let visible = $derived(editing ? filled : filled.filter((f) => f.cards.length > 0));

  function save(next: HomeSection[]) {
    allSections = { ...allSections, [root]: next };
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(allSections));
    } catch {
      // The layout lasts for this session only.
    }
  }

  function update(id: string, patch: Partial<HomeSection>) {
    save(sections.map((s) => (s.id === id ? { ...s, ...patch } : s)));
  }

  function move(id: string, delta: -1 | 1) {
    const i = sections.findIndex((s) => s.id === id);
    const j = i + delta;
    if (i < 0 || j < 0 || j >= sections.length) return;
    const next = [...sections];
    [next[i], next[j]] = [next[j], next[i]];
    save(next);
  }

  function addSection() {
    const id = `s${Date.now().toString(36)}`;
    save([...sections, { id, kind: 'tag', title: 'Tagged notes', limit: 6, sort: 'default', folder: '', tag: '' }]);
  }

  function resetSections() {
    const { [root]: _removed, ...rest } = allSections;
    allSections = rest;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(rest));
    } catch {
      // ignore
    }
  }

  function folderOf(path: string): string {
    const i = path.lastIndexOf('/');
    return i > 0 ? path.slice(0, i) : '';
  }

  const dateFormat = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' });

  function cardDate(card: NoteCard, kind: HomeSectionKind): string {
    const ms = kind === 'recent-created' ? card.createdMs : card.modifiedMs;
    if (!ms) return '';
    const date = new Date(ms);
    const label = date.getFullYear() === new Date().getFullYear()
      ? dateFormat.format(date)
      : date.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
    return kind === 'recent-created' ? `Created ${label}` : `Edited ${label}`;
  }

  const KINDS = Object.entries(SECTION_KIND_LABELS) as Array<[HomeSectionKind, string]>;
  const SORTS: Array<[HomeSort, string]> = [
    ['default', 'Section order'],
    ['title', 'Title'],
    ['modified', 'Last edited'],
    ['created', 'Created'],
  ];
</script>

<div class="home" aria-label="Home">
  <header class="home-header">
    <h1>{vaultStore.vaultSnapshot?.name ?? 'Home'}</h1>
    <div class="header-actions">
      <Button variant="primary" icon="file-plus" onclick={onNewNote}>New note</Button>
      <IconButton
        icon={editing ? 'check' : 'gear-six'}
        label={editing ? 'Done customizing' : 'Customize Home'}
        pressed={editing}
        onclick={() => (editing = !editing)}
      />
    </div>
  </header>

  {#if noteCardsStore.error}
    <p class="empty" role="alert">{noteCardsStore.error}</p>
  {:else if visible.length === 0 && !editing}
    <p class="empty">
      {noteCardsStore.loading ? 'Loading notes…' : 'Notes you open and edit show up here. Press Ctrl O to find a note by name.'}
    </p>
  {/if}

  {#each visible as { section, cards } (section.id)}
    <section class="home-section" class:hidden-section={section.hidden} aria-label={section.title}>
      <div class="section-head">
        <h2>{section.title}</h2>
        {#if section.kind === 'rediscover' && cards.length}<span class="hint">Untouched for 3 months or more</span>{/if}
      </div>

      {#if editing}
        <div class="editor-row">
          <label>Title <input value={section.title} onchange={(e) => update(section.id, { title: (e.currentTarget as HTMLInputElement).value })} /></label>
          <label>Shows
            <select value={section.kind} onchange={(e) => update(section.id, { kind: (e.currentTarget as HTMLSelectElement).value as HomeSectionKind })}>
              {#each KINDS as [kind, label] (kind)}<option value={kind}>{label}</option>{/each}
            </select>
          </label>
          <label>Cards <input type="number" min="1" max="48" value={section.limit} onchange={(e) => update(section.id, { limit: Math.max(1, Math.min(48, Number((e.currentTarget as HTMLInputElement).value) || 1)) })} /></label>
          <label>Sort
            <select value={section.sort} onchange={(e) => update(section.id, { sort: (e.currentTarget as HTMLSelectElement).value as HomeSort })}>
              {#each SORTS as [sort, label] (sort)}<option value={sort}>{label}</option>{/each}
            </select>
          </label>
          <label>Folder <input value={section.folder} placeholder="All folders" onchange={(e) => update(section.id, { folder: (e.currentTarget as HTMLInputElement).value.trim() })} /></label>
          <label>Tag <input value={section.tag} placeholder={section.kind === 'tag' ? 'Required' : 'Any tag'} onchange={(e) => update(section.id, { tag: (e.currentTarget as HTMLInputElement).value.trim().replace(/^#/, '') })} /></label>
          <div class="row-actions">
            <IconButton icon="caret-left" size="sm" label="Move up" class="rot" onclick={() => move(section.id, -1)} />
            <IconButton icon="caret-right" size="sm" label="Move down" class="rot" onclick={() => move(section.id, 1)} />
            <IconButton icon={section.hidden ? 'eye' : 'minus'} size="sm" label={section.hidden ? 'Show section' : 'Hide section'} onclick={() => update(section.id, { hidden: !section.hidden })} />
            <IconButton icon="trash" size="sm" label="Remove section" onclick={() => save(sections.filter((s) => s.id !== section.id))} />
          </div>
        </div>
      {/if}

      {#if cards.length}
        <ul class="cards">
          {#each cards as card (card.path)}
            <li>
              <button type="button" class="card" title={card.path} onclick={() => void tabsStore.openNoteInTab(card.path)}>
                <span class="card-title">{card.title}</span>
                {#if card.preview}<span class="card-preview">{card.preview}</span>{/if}
                <span class="card-meta">
                  {#if folderOf(card.path)}<span class="meta-item"><Icon name="folder-simple" size={12} />{folderOf(card.path)}</span>{/if}
                  <span class="meta-item">{cardDate(card, section.kind)}</span>
                  {#if card.linkedFrom > 0}<span class="meta-item"><Icon name="link-simple" size={12} />{card.linkedFrom}</span>{/if}
                </span>
                {#if card.tags.length}
                  <span class="card-tags">
                    {#each card.tags.slice(0, 3) as tag (tag)}
                      <span class="tag"><span class="dot" style:background={tagColor(tag)}></span>{tag}</span>
                    {/each}
                  </span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {:else if editing}
        <p class="empty small">No notes match this section yet.</p>
      {/if}
    </section>
  {/each}

  {#if editing}
    {#each sections.filter((s) => s.hidden && !visible.some((v) => v.section.id === s.id)) as section (section.id)}
      <div class="hidden-row">
        <span>{section.title} (hidden)</span>
        <Button onclick={() => update(section.id, { hidden: false })}>Show</Button>
      </div>
    {/each}
    <div class="customize-footer">
      <Button icon="plus" onclick={addSection}>Add section</Button>
      <Button onclick={resetSections}>Restore default sections</Button>
    </div>
  {/if}
</div>

<style>
  .home {
    max-width: 1040px;
    margin: 0 auto;
    padding: var(--space-12) var(--space-6);
  }

  .home-header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin-bottom: var(--space-6);
  }

  h1 {
    flex: 1;
    margin: 0;
    font: 600 var(--fs-32) / 1.2 var(--font-display);
    color: var(--text-primary);
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .home-section {
    margin-bottom: var(--space-6);
  }

  .hidden-section {
    opacity: 0.55;
  }

  .section-head {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }

  h2 {
    margin: 0;
    color: var(--text-muted);
    font: 600 var(--fs-11) / 1 var(--font-ui);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .hint {
    color: var(--text-muted);
    font-size: var(--fs-11);
  }

  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    gap: var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    height: 100%;
    min-height: 112px;
    padding: var(--space-3);
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-card);
    background: var(--surface-raised, var(--surface-app));
    color: var(--text-primary);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: border-color var(--transition-fast), transform var(--transition-fast);
  }

  .card:hover {
    border-color: var(--line-control);
  }

  .card:focus-visible {
    outline: 2px solid var(--focus-ring, var(--accent));
    outline-offset: 2px;
  }

  .card-title {
    font: 600 var(--fs-15) / 1.3 var(--font-display);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .card-preview {
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
    color: var(--text-secondary);
    font-size: var(--fs-13);
    line-height: 1.45;
  }

  .card-meta,
  .card-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    margin-top: auto;
    color: var(--text-muted);
    font-size: var(--fs-11);
  }

  .card-tags {
    margin-top: 0;
    gap: 4px;
  }

  .meta-item,
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .tag {
    padding: 1px 6px;
    border-radius: 8px;
    background: var(--surface-app);
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }

  .editor-row {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: var(--space-2);
    margin-bottom: var(--space-3);
    padding: var(--space-2);
    border: 1px dashed var(--line-control);
    border-radius: var(--radius-card);
    font-size: var(--fs-12);
    color: var(--text-secondary);
  }

  .editor-row label {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .editor-row input,
  .editor-row select {
    height: 28px;
    min-width: 0;
    width: 140px;
    padding: 0 6px;
    border: 1px solid var(--line-control);
    border-radius: var(--radius-control);
    background: var(--surface-app);
    color: var(--text-primary);
    font: inherit;
  }

  .editor-row input[type='number'] {
    width: 64px;
  }

  .row-actions {
    display: flex;
    gap: 2px;
    margin-left: auto;
  }

  .row-actions :global(.rot svg) {
    transform: rotate(90deg);
  }

  .hidden-row,
  .customize-footer {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
    color: var(--text-secondary);
    font-size: var(--fs-13);
  }

  .hidden-row span {
    flex: 1;
  }

  .empty {
    color: var(--text-muted);
    font-size: var(--fs-13);
  }

  .empty.small {
    margin: 0;
    font-size: var(--fs-12);
  }

  @media (max-width: 640px) {
    .home {
      padding: var(--space-6) var(--space-4);
    }
  }
</style>
