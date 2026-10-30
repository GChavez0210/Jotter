<script lang="ts">
  import { onMount } from 'svelte';
  import Button from './ui/Button.svelte';
  import Dialog from './ui/Dialog.svelte';
  import { askIndexStatus, askIndexVault, askVault } from './ipc';
  import { AI_PROVIDER_LABELS, aiStore } from './stores/ai.svelte';
  import { m } from './i18n.svelte';
  import type { AskAnswer, AskIndexStatus } from './types';

  interface Props {
    onOpenNote: (path: string, line: number) => void;
    onOpenSettings: () => void;
    onClose: () => void;
  }

  let { onOpenNote, onOpenSettings, onClose }: Props = $props();

  const config = $derived(aiStore.status?.config);
  const enabled = $derived(!!config?.enabled && !!config.askVaultEnabled);

  let status = $state<AskIndexStatus | null>(null);
  let question = $state('');
  let answer = $state<AskAnswer | null>(null);
  let notice = $state('');
  let error = $state('');
  let phase = $state<'idle' | 'indexing' | 'asking'>('idle');

  async function refreshStatus() {
    try {
      status = await askIndexStatus();
    } catch (err) {
      error = String(err);
    }
  }

  onMount(async () => {
    await aiStore.ensureLoaded();
    if (enabled) await refreshStatus();
  });

  async function index() {
    if (!enabled || phase !== 'idle') return;
    error = '';
    notice = '';
    phase = 'indexing';
    try {
      const report = await askIndexVault();
      notice = report.removedNotes && !report.embeddedNotes
        ? m.ask_index_cleaned({ count: report.removedNotes })
        : m.ask_indexed({ notes: report.embeddedNotes, passages: report.passages });
      await refreshStatus();
    } catch (err) {
      error = String(err);
    } finally {
      phase = 'idle';
    }
  }

  async function ask() {
    if (!enabled || !question.trim() || phase !== 'idle') return;
    error = '';
    notice = '';
    answer = null;
    phase = 'asking';
    try {
      answer = await askVault(question);
    } catch (err) {
      error = String(err);
    } finally {
      phase = 'idle';
    }
  }

  const embeddingLabel = $derived(config ? AI_PROVIDER_LABELS[config.embeddingProvider] : '');
</script>

<Dialog label={m.ask_title()} variant="panel" {onClose}>
  <div class="ask">
    <h2 class="title">{m.ask_title()}</h2>

    {#if !enabled}
      <p class="muted">{m.ask_off()}</p>
      <div class="actions">
        <Button onclick={onClose}>{m.common_close()}</Button>
        <Button variant="primary" onclick={onOpenSettings}>{m.ask_open_settings()}</Button>
      </div>
    {:else}
      {#if status}
        <div class="index muted" role="status">
          {#if status.notesToEmbed > 0 || status.notesToRemove > 0}
            <span>
              {status.notesToEmbed > 0
                ? m.ask_index_needed({ count: status.notesToEmbed, total: status.notesTotal, provider: embeddingLabel })
                : m.ask_cleanup_needed({ count: status.notesToRemove })}
            </span>
            <Button disabled={phase !== 'idle'} onclick={() => void index()}>
              {phase === 'indexing' ? m.ask_indexing() : status.notesToEmbed > 0
                ? m.ask_index_button({ count: status.notesToEmbed }) : m.ask_cleanup_button()}
            </Button>
          {:else}
            <span>{m.ask_index_current({ total: status.notesTotal })}</span>
          {/if}
        </div>
      {/if}

      <form
        class="question"
        onsubmit={(e) => {
          e.preventDefault();
          void ask();
        }}
      >
        <label class="muted" for="ask-question">{m.ask_question_label()}</label>
        <textarea
          id="ask-question"
          rows="3"
          placeholder={m.ask_question_placeholder()}
          bind:value={question}
          onkeydown={(e) => {
            if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              void ask();
            }
          }}
        ></textarea>
        <div class="actions">
          <Button onclick={onClose}>{m.common_close()}</Button>
          <Button type="submit" variant="primary" icon="sparkle" disabled={phase !== 'idle' || !question.trim()}>
            {m.ask_submit()}
          </Button>
        </div>
      </form>

      {#if phase === 'asking'}<div class="muted" role="status">{m.ask_searching()}</div>{/if}
      {#if notice}<div class="muted" role="status">{notice}</div>{/if}
      {#if error}<div class="error" role="alert">{error}</div>{/if}

      {#if answer}
        <div class="answer">{answer.answer}</div>
        {#if answer.sources.length}
          <div class="muted">{m.ask_sources()}</div>
          <ul class="sources">
            {#each answer.sources as source}
              <li>
                <button type="button" class="source" onclick={() => onOpenNote(source.chunk.path, source.chunk.line)}>
                  {source.chunk.path}:{source.chunk.line}
                  {#if source.chunk.heading}<span class="muted"> · {source.chunk.heading}</span>{/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
    {/if}
  </div>
</Dialog>

<style>
  .ask {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    box-sizing: border-box;
    width: min(560px, calc(100vw - 24px));
    max-height: calc(100dvh - 24px);
    overflow: auto;
    padding: var(--space-4);
  }

  .title {
    margin: 0;
    font-size: 1.1em;
  }

  .muted {
    color: var(--text-muted);
  }

  .index {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .question {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  textarea {
    box-sizing: border-box;
    width: 100%;
    resize: vertical;
    padding: var(--space-2);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-control);
    background: var(--surface-field);
    color: var(--text-primary);
    font: inherit;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }

  .answer {
    white-space: pre-wrap;
    user-select: text;
  }

  .error {
    color: var(--text-danger);
  }

  .sources {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .source {
    padding: var(--space-1) 0;
    border: 0;
    background: transparent;
    color: var(--accent);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .source:hover {
    text-decoration: underline;
  }
</style>
