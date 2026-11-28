<script lang="ts">
  import { onMount } from 'svelte';
  import AiAccess from './AiAccess.svelte';
  import Button from './ui/Button.svelte';
  import Dialog from './ui/Dialog.svelte';
  import IconButton from './ui/IconButton.svelte';
  import { citeAnswer } from './askCitations';
  import { askIndexStatus, askIndexVault, askVault } from './ipc';
  import { aiDestination, aiStore } from './stores/ai.svelte';
  import { m } from './i18n.svelte';
  import type { AiAccessOverview, AskAnswer, AskIndexStatus } from './types';

  interface Props {
    onOpenNote: (path: string, line: number) => void;
    onOpenSettings: () => void;
    onClose: () => void;
  }

  let { onOpenNote, onOpenSettings, onClose }: Props = $props();

  const config = $derived(aiStore.status?.config);
  const enabled = $derived(!!config?.enabled && !!config.askVaultEnabled);

  let status = $state<AskIndexStatus | null>(null);
  let overview = $state<AiAccessOverview | null>(null);
  const sessionId = crypto.randomUUID();
  let question = $state('');
  /** The question the current answer, or the pending request, belongs to. */
  let asked = $state('');
  let answer = $state<AskAnswer | null>(null);
  let notice = $state('');
  let error = $state('');
  let phase = $state<'idle' | 'indexing' | 'asking'>('idle');

  const cited = $derived(answer ? citeAnswer(answer.answer, answer.sources) : null);
  const pending = $derived(!!status && (status.notesToEmbed > 0 || status.notesToRemove > 0));
  const destination = $derived.by(() => {
    if (!config) return '';
    const search = aiDestination(config, config.embeddingProvider, config.embeddingBaseUrl ?? undefined);
    return config.embeddingProvider === config.provider && config.embeddingBaseUrl == null
      ? m.ask_sent_to({ destination: search })
      : m.ask_sent_split({ search, answer: aiDestination(config, config.provider) });
  });
  let accessGeneration = 0;

  function accessChanged() {
    accessGeneration++;
    answer = null;
    void refreshStatus();
  }

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
    const sent = question;
    error = '';
    notice = '';
    answer = null;
    asked = sent.trim();
    phase = 'asking';
    const generation = accessGeneration;
    try {
      const result = await askVault(sent, sessionId);
      if (generation === accessGeneration) {
        answer = result;
        if (question === sent) question = '';
      }
    } catch (err) {
      if (generation === accessGeneration) error = String(err);
    } finally {
      phase = 'idle';
    }
  }

  function focus(node: HTMLElement) {
    node.focus();
  }
</script>

<Dialog label={m.ask_title()} variant="panel" {onClose}>
  <div class="ask">
    <header>
      <h2>{m.ask_title()}</h2>
      <IconButton icon="x" label={m.common_close()} onclick={onClose} />
    </header>

    {#if !enabled}
      <div class="off">
        <p class="muted">{m.ask_off()}</p>
        <Button variant="primary" onclick={onOpenSettings}>{m.ask_open_settings()}</Button>
      </div>
    {:else}
      <div class="body">
        <section class="notes" aria-labelledby="ask-notes">
          <h3 id="ask-notes">{m.ai_access_title()}</h3>
          <AiAccess
            fill
            onOverview={(next, changed) => {
              overview = next;
              if (changed) accessChanged();
            }}
          />
          <div class="index muted">
            <div role="status">
              {#if overview}
                <div>
                  {m.ai_access_count({ shared: overview.notes.filter((note) => note.shared).length, total: overview.notes.length })}
                </div>
              {/if}
              {#if status}
                <div>
                  {#if status.notesToEmbed > 0}
                    {m.ask_need_indexing({ count: status.notesToEmbed })}
                  {:else if status.notesToRemove > 0}
                    {m.ask_cleanup_needed({ count: status.notesToRemove })}
                  {:else if status.notesTotal === 0}
                    {m.ask_index_empty()}
                  {:else}
                    {m.ask_index_ready()}
                  {/if}
                </div>
              {/if}
              {#if notice}<div>{notice}</div>{/if}
            </div>
            <div>{destination}</div>
            {#if pending && status}
              <Button disabled={phase !== 'idle'} onclick={() => void index()}>
                {phase === 'indexing' ? m.ask_indexing()
                  : status.notesToEmbed > 0 ? m.ask_update_index() : m.ask_cleanup_button()}
              </Button>
            {/if}
          </div>
        </section>

        <div class="chat">
          <div class="thread">
            {#if asked}<div class="asked">{asked}</div>{/if}
            {#if phase === 'asking'}<div class="muted" role="status">{m.ask_searching()}</div>{/if}
            {#if error}<div class="error" role="alert">{error}</div>{/if}
            <div aria-live="polite">
              {#if cited}
                <!-- The answer keeps its own line breaks, so the markup adds none. -->
                <div class="answer">{#each cited.parts as part}{#if typeof part === 'string'}{part}{:else}<button
                        type="button"
                        class="cite"
                        aria-label={m.ask_source({ number: part.number, location: part.location })}
                        title={part.location}
                        onclick={() => onOpenNote(part.chunk.path, part.chunk.line)}>{part.number}</button>{/if}{/each}</div>
              {/if}
            </div>
            {#if cited?.sources.length}
              <ul class="sources" aria-label={m.ask_sources()}>
                {#each cited.sources as source (source.location)}
                  <li>
                    <button
                      type="button"
                      class="source"
                      title={source.chunk.heading || undefined}
                      onclick={() => onOpenNote(source.chunk.path, source.chunk.line)}
                    >{source.number} · {source.location}</button>
                  </li>
                {/each}
              </ul>
            {/if}
          </div>

          <form
            class="composer"
            onsubmit={(e) => {
              e.preventDefault();
              void ask();
            }}
          >
            <label class="sr-only" for="ask-question">{m.ask_question_label()}</label>
            <textarea
              id="ask-question"
              rows="2"
              placeholder={asked ? m.ask_question_another() : m.ask_question_placeholder()}
              bind:value={question}
              use:focus
              onkeydown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
                  e.preventDefault();
                  void ask();
                }
              }}
            ></textarea>
            <Button type="submit" variant="primary" icon="sparkle" disabled={phase !== 'idle' || !question.trim()}>
              {m.ask_submit()}
            </Button>
          </form>
        </div>
      </div>
    {/if}
  </div>
</Dialog>

<style>
  .ask {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--line-subtle);
  }

  h2 {
    margin: 0;
    font-size: 1.1em;
  }

  .off {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    padding: var(--space-4);
    text-align: center;
  }

  .body {
    display: grid;
    flex: 1;
    grid-template-columns: minmax(220px, 2fr) 3fr;
    grid-template-rows: minmax(0, 1fr);
    min-height: 0;
  }

  .notes,
  .chat {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-width: 0;
    min-height: 0;
    padding: var(--space-3) var(--space-4);
  }

  .notes {
    overflow: auto;
    border-right: 1px solid var(--line-subtle);
  }

  h3 {
    margin: 0;
    color: var(--text-muted);
    font: 600 var(--fs-11) / 1.3 var(--font-ui);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .muted {
    color: var(--text-muted);
  }

  .index {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: auto;
    padding-top: var(--space-3);
    border-top: 1px solid var(--line-subtle);
    font-size: var(--fs-12);
    overflow-wrap: anywhere;
  }

  .thread {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 0;
    overflow: auto;
  }

  .asked {
    flex: none;
    align-self: flex-end;
    max-width: 85%;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--line-subtle);
    border-radius: var(--radius-card);
    background: var(--surface-hover);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }

  .answer {
    line-height: 2;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }

  .cite {
    min-width: 24px;
    min-height: 24px;
    margin: 0 2px;
    padding: 0 var(--space-1);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--accent);
    font: var(--fs-11) var(--font-mono);
    user-select: none;
    cursor: pointer;
  }

  .sources {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .source {
    min-width: 24px;
    min-height: 28px;
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--accent);
    font: inherit;
    font-size: var(--fs-12);
    text-align: left;
    overflow-wrap: anywhere;
    cursor: pointer;
  }

  .cite:hover,
  .source:hover {
    background: var(--surface-hover);
  }

  .composer {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-1);
    padding: var(--space-2);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-card);
    background: var(--surface-field);
  }

  .composer:focus-within {
    border-color: var(--accent);
  }

  .composer :global(button) {
    align-self: flex-end;
  }

  textarea {
    min-width: 0;
    resize: none;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text-primary);
    font: inherit;
  }

  .error {
    color: var(--text-danger);
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  /* Too narrow for two columns: the answer comes first, and the whole body scrolls. */
  @media (max-width: 640px) {
    .body {
      display: flex;
      flex-direction: column-reverse;
      justify-content: flex-end;
      overflow: auto;
    }

    .notes {
      flex: none;
      max-height: 320px;
      border-top: 1px solid var(--line-subtle);
      border-right: 0;
    }

    .chat {
      flex: 1 0 auto;
      min-height: 240px;
    }
  }
</style>
