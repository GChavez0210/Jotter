<script lang="ts">
  import { onMount } from 'svelte';
  import Button from './ui/Button.svelte';
  import Dialog from './ui/Dialog.svelte';
  import Segmented from './ui/Segmented.svelte';
  import { copyToClipboard } from './ipc';
  import { AI_PROVIDER_LABELS, REWRITE_STYLE_LABELS, aiStore, type AiOperationKind } from './stores/ai.svelte';
  import type { AiPlacement, AiScope, AiTarget } from './aiText';
  import type { RewriteStyle } from './types';

  interface Props {
    hasSelection: boolean;
    captureTarget: (scope: AiScope) => AiTarget | null;
    applyResult: (target: AiTarget, text: string, placement: AiPlacement) => boolean;
    onOpenSettings: () => void;
    onClose: () => void;
  }

  let { hasSelection, captureTarget, applyResult, onOpenSettings, onClose }: Props = $props();

  const OPERATIONS = [
    { value: 'rewrite', label: 'Rewrite' },
    { value: 'translate', label: 'Translate' },
    { value: 'summarize', label: 'Summarize' },
  ] as const satisfies readonly { value: AiOperationKind; label: string }[];

  const LANGUAGES = [
    'English', 'Spanish', 'French', 'German', 'Italian', 'Portuguese', 'Dutch', 'Polish',
    'Russian', 'Ukrainian', 'Turkish', 'Arabic', 'Hindi', 'Japanese', 'Korean',
    'Simplified Chinese', 'Traditional Chinese',
  ];

  let scope = $state<AiScope>('note');
  let target = $state<AiTarget | null>(null);
  let notice = $state('');
  let resultEl: HTMLTextAreaElement | undefined = $state();

  const provider = $derived(aiStore.status?.config.provider ?? 'openai-compatible');
  const endpoint = $derived(aiStore.endpoint(provider));
  const destination = $derived.by(() => {
    try {
      return endpoint?.baseUrl ? new URL(endpoint.baseUrl).host : '';
    } catch {
      return '';
    }
  });
  const ready = $derived(aiStore.enabled && !!endpoint?.baseUrl && !!endpoint?.model);
  const running = $derived(aiStore.run.phase === 'running');
  const result = $derived(aiStore.run.phase === 'done' ? aiStore.run.result : null);
  const preferredPlacement = $derived<AiPlacement>(
    aiStore.panelOperation === 'summarize' ? 'below' : 'replace'
  );

  onMount(() => {
    scope = hasSelection ? 'selection' : 'note';
  });

  function run() {
    notice = '';
    let captured: AiTarget | null;
    try {
      captured = captureTarget(scope);
    } catch (err) {
      notice = err instanceof Error ? err.message : String(err);
      return;
    }
    if (!captured) {
      notice = scope === 'selection' ? 'Select some text first.' : 'The note has no text to send.';
      return;
    }
    if (aiStore.panelOperation === 'translate' && !aiStore.targetLanguage.trim()) {
      notice = 'Choose a language to translate into.';
      return;
    }
    target = captured;
    void aiStore.start(aiStore.operation(), captured.text);
  }

  function apply(placement: AiPlacement) {
    if (!result || !target) return;
    if (applyResult(target, result.text, placement)) {
      onClose();
    } else {
      notice = 'The note changed after the request started, so Jotter did not apply the result. Run the command again.';
    }
  }

  async function copyResult() {
    if (!result) return;
    try {
      await copyToClipboard({ text: result.text, mode: 'markdown_source' });
      notice = 'Copied the result.';
    } catch (err) {
      notice = `Copy failed: ${err}`;
    }
  }

  function usageLabel() {
    if (!result) return '';
    const { inputTokens, outputTokens } = result.usage;
    const tokens = inputTokens !== null && outputTokens !== null ? ` · ${inputTokens} in, ${outputTokens} out tokens` : '';
    return `${result.model} via ${AI_PROVIDER_LABELS[result.provider]}${tokens}`;
  }

  function handleKeydown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
      event.preventDefault();
      if (result) apply(preferredPlacement);
      else if (!running && ready) run();
    }
  }

  $effect(() => {
    if (result) resultEl?.focus();
  });
</script>

<Dialog label="AI" variant="panel" {onClose} onkeydown={handleKeydown}>
  <div class="ai-panel">
    <header class="ai-head">
      <h2>AI</h2>
      <Segmented
        label="Command"
        options={OPERATIONS}
        value={aiStore.panelOperation}
        onchange={(kind) => {
          aiStore.cancel();
          aiStore.panelOperation = kind;
          aiStore.run = { phase: 'idle' };
          notice = '';
        }}
      />
    </header>

    {#if !aiStore.status}
      <p class="muted">{aiStore.loadError || 'Loading AI settings…'}</p>
    {:else if !ready}
      <div class="setup">
        <p>
          {aiStore.enabled
            ? `Enter a base URL and model for ${AI_PROVIDER_LABELS[provider]} first.`
            : 'AI is off. Turn it on and choose a provider to summarize, rewrite, and translate notes.'}
        </p>
        <Button variant="primary" icon="gear-six" onclick={onOpenSettings}>Open AI settings</Button>
      </div>
    {:else}
      <div class="options">
        <Segmented
          label="Text to send"
          options={[
            { value: 'selection', label: 'Selection' },
            { value: 'note', label: 'Whole note' },
          ]}
          value={scope}
          onchange={(next) => (scope = next)}
        />
        {#if aiStore.panelOperation === 'rewrite'}
          <label class="field">
            <span>Style</span>
            <select
              value={aiStore.rewriteStyle}
              onchange={(e) => aiStore.setRewriteStyle((e.currentTarget as HTMLSelectElement).value as RewriteStyle)}
            >
              {#each Object.entries(REWRITE_STYLE_LABELS) as [value, label] (value)}
                <option {value}>{label}</option>
              {/each}
            </select>
          </label>
        {:else if aiStore.panelOperation === 'translate'}
          <label class="field">
            <span>Into</span>
            <input
              type="text"
              list="ai-languages"
              value={aiStore.targetLanguage}
              maxlength="60"
              oninput={(e) => aiStore.setTargetLanguage((e.currentTarget as HTMLInputElement).value)}
            />
            <datalist id="ai-languages">
              {#each LANGUAGES as lang (lang)}<option value={lang}></option>{/each}
            </datalist>
          </label>
        {/if}
      </div>

      {#if aiStore.panelOperation === 'rewrite' && aiStore.rewriteStyle === 'polish'}
        <p class="muted">
          Polish follows the writing style in Settings › AI.
          <button type="button" class="link" onclick={onOpenSettings}>Edit writing style</button>
        </p>
      {/if}

      <p class="muted">
        Sends the {scope === 'selection' ? 'selected text' : 'note, without its properties,'} to
        <strong>{destination}</strong> ({AI_PROVIDER_LABELS[provider]}, {endpoint?.model}). Code,
        links, and math stay unchanged.
      </p>

      <div class="result-area">
        {#if aiStore.run.phase === 'running'}
          <div class="status" role="status">Waiting for {AI_PROVIDER_LABELS[provider]}…</div>
        {:else if aiStore.run.phase === 'error'}
          <div class="error" role="alert">{aiStore.run.message}</div>
        {:else if result}
          <textarea
            bind:this={resultEl}
            class="result"
            readonly
            aria-label="Result preview"
            value={result.text}
          ></textarea>
          <div class="muted small">{usageLabel()}</div>
        {:else}
          <div class="status muted">Run the command to preview the result here. Nothing changes in the note until you apply it.</div>
        {/if}
      </div>
    {/if}

    {#if notice}<div class="notice" role="status">{notice}</div>{/if}

    <footer class="actions">
      {#if result}
        <Button onclick={() => void copyResult()} icon="copy">Copy</Button>
        <Button onclick={() => aiStore.run = { phase: 'idle' }}>Discard</Button>
        <Button
          variant={preferredPlacement === 'below' ? 'primary' : 'default'}
          onclick={() => apply('below')}
        >Insert below</Button>
        <Button
          variant={preferredPlacement === 'replace' ? 'primary' : 'default'}
          onclick={() => apply('replace')}
        >Replace</Button>
      {:else if running}
        <Button onclick={() => aiStore.cancel()} icon="stop">Cancel</Button>
      {:else}
        <Button onclick={onClose}>Close</Button>
        {#if ready}
          <Button variant="primary" icon="sparkle" onclick={run}>Run</Button>
        {/if}
      {/if}
    </footer>
  </div>
</Dialog>

<style>
  .ai-panel {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 0;
    padding: var(--space-5) var(--space-6);
  }

  .ai-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  h2 {
    margin: 0;
    font: 600 var(--fs-24) / 1.25 var(--font-display);
  }

  .options {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: var(--space-4);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 220px;
    color: var(--text-secondary);
    font-size: var(--fs-12);
  }

  .field select,
  .field input {
    min-height: var(--control-min-height);
    padding: 0 var(--space-2);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-control);
    background: var(--surface-field);
    color: var(--text-primary);
    font: var(--fs-13) var(--font-ui);
  }

  .muted {
    margin: 0;
    color: var(--text-muted);
    font-size: var(--fs-12);
    line-height: var(--leading-normal);
  }

  .muted strong {
    color: var(--text-secondary);
  }

  .small {
    font-size: var(--fs-11);
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    text-decoration: underline;
    cursor: pointer;
  }

  .setup {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-3);
  }

  .setup p {
    margin: 0;
    color: var(--text-secondary);
  }

  .result-area {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-1);
    min-height: 0;
  }

  .status,
  .error {
    padding: var(--space-4);
    border: 1px dashed var(--line-control);
    border-radius: var(--radius-card);
    font-size: var(--fs-13);
  }

  .error {
    border-style: solid;
    color: var(--text-danger);
  }

  .result {
    flex: 1;
    min-height: 160px;
    padding: var(--space-3);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-card);
    background: var(--surface-editor);
    color: var(--text-primary);
    font: var(--fs-13) / 1.6 var(--font-mono);
    resize: none;
  }

  .notice {
    color: var(--text-secondary);
    font-size: var(--fs-12);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
