<script lang="ts">
  /**
   * The timeline of a note's stored versions, with what each one would change and a way back
   * to it. The diff compares the selected version with the note as it is now, so it reads as
   * "restoring this would do the following".
   */
  import Button from './ui/Button.svelte';
  import Dialog from './ui/Dialog.svelte';
  import Icon from './ui/Icon.svelte';
  import Segmented from './ui/Segmented.svelte';
  import { listNoteVersions, readNoteVersion, restoreNoteVersion } from './ipc';
  import { collapseUnchanged, diffLines, diffSummary, type DiffRow } from './diff';
  import { clipboardStore } from './stores/clipboard.svelte';
  import { settingsStore } from './stores/settings.svelte';
  import { tabsStore } from './stores/tabs.svelte';
  import { vaultStore } from './stores/vault.svelte';
  import { i18n, m } from './i18n.svelte';
  import type { NoteVersion } from './types';
  import { sameNotePath } from './notePaths';

  interface Props {
    relPath: string;
    onClose: () => void;
    /** Puts restored text into the pane showing the note, so the editor matches the file. */
    onRestored: (relPath: string, content: string) => void;
  }

  let { relPath, onClose, onRestored }: Props = $props();

  let versions = $state<NoteVersion[]>([]);
  let selectedId = $state<string | null>(null);
  let selectedText = $state<string | null>(null);
  let view = $state<'changes' | 'text'>('changes');
  let loading = $state(true);
  let busy = $state(false);
  let error = $state('');

  let title = $derived(relPath.split('/').pop()?.replace(/\.md$/i, '') ?? relPath);

  /** The note as it is now, including edits that have not reached disk yet. */
  let current = $derived(
    tabsStore.openTabs.find((t) => sameNotePath(t.relPath, relPath))?.content ?? ''
  );

  let lines = $derived(selectedText === null ? [] : diffLines(current, selectedText));
  let rows = $derived<DiffRow[]>(collapseUnchanged(lines));
  let summary = $derived(diffSummary(lines));

  async function load() {
    loading = true;
    error = '';
    try {
      versions = await listNoteVersions(relPath);
      if (versions.length > 0) await select(versions[0].id);
    } catch (err) {
      error = m.version_failed({ error: err instanceof Error ? err.message : String(err) });
    } finally {
      loading = false;
    }
  }

  async function select(id: string) {
    selectedId = id;
    selectedText = null;
    try {
      const text = await readNoteVersion(relPath, id);
      // A click on another version while this one loaded wins.
      if (selectedId === id) selectedText = text;
    } catch (err) {
      error = m.version_failed({ error: err instanceof Error ? err.message : String(err) });
    }
  }

  async function restore() {
    if (!selectedId || busy) return;
    busy = true;
    error = '';
    try {
      const id = selectedId;
      // Pending edits would otherwise overwrite the restored text a moment later.
      if (!(await tabsStore.flushAutosaveFor(relPath))) {
        error = m.version_save_failed();
        return;
      }
      const doc = await restoreNoteVersion(relPath, id);
      onRestored(doc.relPath, doc.content);
      vaultStore.bumpIndexVersion();
      clipboardStore.setStatus(m.version_restored({ time: describe(Number(id)) }));
      await load();
    } catch (err) {
      error = m.version_failed({ error: err instanceof Error ? err.message : String(err) });
    } finally {
      busy = false;
    }
  }

  function describe(ms: number): string {
    const date = new Date(ms);
    const today = new Date();
    const time = date.toLocaleTimeString(i18n.locale, { hour: '2-digit', minute: '2-digit' });
    if (date.toDateString() === today.toDateString()) return m.version_today({ time });
    const yesterday = new Date(today);
    yesterday.setDate(today.getDate() - 1);
    if (date.toDateString() === yesterday.toDateString()) return m.version_yesterday({ time });
    return `${date.toLocaleDateString(i18n.locale, { month: 'short', day: 'numeric' })} ${time}`;
  }

  function sizeLabel(bytes: number): string {
    return bytes < 1024
      ? m.version_bytes({ count: bytes })
      : m.version_kilobytes({ count: Math.round(bytes / 1024) });
  }

  $effect(() => {
    void relPath;
    void load();
  });
</script>

<Dialog label={m.version_title({ note: title })} variant="panel" {onClose}>
  <header class="head">
    <div class="heading">
      <h2>{m.version_title({ note: title })}</h2>
      <p class="hint">{m.version_hint()}</p>
    </div>
    <Button icon="x" onclick={onClose}>{m.common_close()}</Button>
  </header>

  <div class="body">
    <nav class="timeline" aria-label={m.version_timeline()}>
      {#if loading}
        <p class="empty">{m.version_loading()}</p>
      {:else if versions.length === 0}
        <p class="empty">{m.version_none()}</p>
      {:else}
        <ul>
          {#each versions as version (version.id)}
            <li>
              <button
                type="button"
                class="entry"
                class:selected={version.id === selectedId}
                aria-current={version.id === selectedId ? 'true' : undefined}
                onclick={() => void select(version.id)}
              >
                <Icon name="clock-counter-clockwise" size={14} />
                <span class="when">{describe(version.savedAtMs)}</span>
                <span class="size">{sizeLabel(version.bytes)}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </nav>

    <section class="detail" aria-label={m.version_changes()}>
      {#if error}
        <p class="error" role="alert">{error}</p>
      {/if}
      {#if selectedId && selectedText !== null}
        <div class="detail-head">
          <Segmented
            label={m.version_view()}
            value={view}
            options={[
              { value: 'changes', label: m.version_changes(), icon: 'git-diff' },
              { value: 'text', label: m.version_full_text(), icon: 'file-text' },
            ]}
            onchange={(next) => (view = next)}
          />
          <span class="counts">
            {m.version_counts({ added: summary.added, removed: summary.removed })}
          </span>
          <Button
            variant="primary"
            icon="arrow-bend-up-left"
            disabled={busy || summary.added + summary.removed === 0}
            onclick={() => void restore()}
          >
            {m.version_restore()}
          </Button>
        </div>

        {#if view === 'text'}
          <pre class="text-view">{selectedText}</pre>
        {:else if summary.added + summary.removed === 0}
          <p class="empty">{m.version_same()}</p>
        {:else}
          <div class="diff" role="list">
            {#each rows as row, index (index)}
              {#if row.kind === 'gap'}
                <div class="gap" role="listitem">{m.version_unchanged({ count: row.count })}</div>
              {:else}
                <div class="row {row.kind}" role="listitem">
                  <span class="sr-only">{row.kind === 'add' ? m.version_line_added() : row.kind === 'del' ? m.version_line_removed() : m.version_line_unchanged()}</span>
                  <span class="marker" aria-hidden="true"
                    >{row.kind === 'add' ? '+' : row.kind === 'del' ? '−' : ' '}</span
                  >
                  <span class="line">{row.text || ' '}</span>
                </div>
              {/if}
            {/each}
          </div>
        {/if}
      {:else if !loading && versions.length > 0}
        <p class="empty">{m.version_loading()}</p>
      {:else if !loading}
        <p class="empty">{m.version_none_hint()}</p>
      {/if}
    </section>
  </div>
</Dialog>

<style>
  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  .head {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4) var(--space-3);
    border-bottom: 1px solid var(--line-subtle);
  }

  .heading {
    flex: 1;
    min-width: 0;
  }

  h2 {
    margin: 0;
    font: 600 var(--fs-15) / var(--leading-tight) var(--font-ui);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hint {
    margin: 2px 0 0;
    color: var(--text-muted);
    font-size: var(--fs-12);
  }

  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .timeline {
    flex: none;
    width: 212px;
    min-height: 0;
    overflow-y: auto;
    border-right: 1px solid var(--line-subtle);
    padding: var(--space-2);
  }

  .timeline ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .entry {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: 6px var(--space-2);
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .entry:hover {
    background: var(--surface-hover);
    color: var(--text-primary);
  }

  .entry.selected {
    background: var(--accent-soft);
    color: var(--text-primary);
  }

  .when {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .size {
    flex: none;
    color: var(--text-muted);
    font-size: var(--fs-11);
  }

  .detail {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .detail-head {
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--line-subtle);
  }

  .counts {
    flex: 1;
    color: var(--text-muted);
    font-size: var(--fs-12);
  }

  .diff,
  .text-view {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: var(--space-2) 0;
    overflow: auto;
    font-family: var(--font-mono);
    font-size: var(--fs-12);
    line-height: 1.55;
  }

  .text-view {
    padding: var(--space-3);
    white-space: pre-wrap;
    color: var(--text-secondary);
  }

  .row {
    display: flex;
    gap: var(--space-2);
    padding: 0 var(--space-3);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .row.same {
    color: var(--text-muted);
  }

  .row.add {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--text-primary);
  }

  .row.del {
    background: color-mix(in srgb, var(--text-danger) 12%, transparent);
    color: var(--text-secondary);
    text-decoration: line-through;
    text-decoration-color: color-mix(in srgb, var(--text-danger) 50%, transparent);
  }

  .marker {
    flex: none;
    width: 1ch;
    color: var(--text-muted);
  }

  .line {
    flex: 1;
    min-width: 0;
  }

  .gap {
    padding: 2px var(--space-3);
    color: var(--text-muted);
    font-size: var(--fs-11);
    font-family: var(--font-ui);
    background: var(--surface-app);
  }

  .empty,
  .error {
    margin: 0;
    padding: var(--space-6) var(--space-4);
    color: var(--text-muted);
    text-align: center;
  }

  .error {
    padding: var(--space-3) var(--space-4);
    color: var(--text-danger);
    text-align: left;
  }
</style>
