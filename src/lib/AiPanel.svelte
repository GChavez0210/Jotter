<script lang="ts">
  import { onMount } from 'svelte';
  import Button from './ui/Button.svelte';
  import { aiPopoverPosition, clampAiPopoverPosition } from './aiPopover';
  import Segmented from './ui/Segmented.svelte';
  import { copyToClipboard } from './ipc';
  import { AI_PROVIDER_LABELS, REWRITE_ACTIONS, REWRITE_TONES, REWRITE_STYLE_LABELS, aiStore, type AiOperationKind } from './stores/ai.svelte';
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

  const screens = $state<Record<AiOperationKind, { scope: AiScope; target: AiTarget | null; notice: string }>>({
    rewrite: { scope: 'note', target: null, notice: '' },
    translate: { scope: 'note', target: null, notice: '' },
    summarize: { scope: 'note', target: null, notice: '' },
  });
  const screen = $derived(screens[aiStore.panelOperation]);
  let resultEl: HTMLTextAreaElement | undefined = $state();
  let menuEl: HTMLDivElement | undefined = $state();
  let position = $state({ left: 12, top: 12 });
  let moved = false;
  let drag = $state<{ id: number; x: number; y: number; left: number; top: number } | null>(null);

  function clampPosition(next: { left: number; top: number }) {
    if (!menuEl) return next;
    return clampAiPopoverPosition(next, menuEl.getBoundingClientRect(), {
      width: window.innerWidth, height: window.innerHeight,
    });
  }

  function reposition() {
    if (!menuEl) return;
    if (moved) {
      position = clampPosition(position);
      return;
    }
    position = aiPopoverPosition(menuEl.getBoundingClientRect(), {
      width: window.innerWidth, height: window.innerHeight,
    });
  }

  function startDrag(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    moved = true;
    drag = { id: event.pointerId, x: event.clientX, y: event.clientY, ...position };
  }

  function moveDrag(event: PointerEvent) {
    if (!drag || event.pointerId !== drag.id) return;
    position = clampPosition({
      left: drag.left + event.clientX - drag.x,
      top: drag.top + event.clientY - drag.y,
    });
  }

  function moveWithKeyboard(event: KeyboardEvent) {
    const step = event.shiftKey ? 10 : 1;
    const dx = event.key === 'ArrowLeft' ? -step : event.key === 'ArrowRight' ? step : 0;
    const dy = event.key === 'ArrowUp' ? -step : event.key === 'ArrowDown' ? step : 0;
    if (!dx && !dy && event.key !== 'Home') return;
    event.preventDefault();
    event.stopPropagation();
    moved = event.key !== 'Home';
    if (moved) position = clampPosition({ left: position.left + dx, top: position.top + dy });
    else reposition();
  }

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
    for (const tab of Object.values(screens)) tab.scope = hasSelection ? 'selection' : 'note';
    function dismissOutside(event: PointerEvent) {
      if (event.target instanceof Node && !menuEl?.contains(event.target)) onClose();
    }
    const observer = new ResizeObserver(reposition);
    if (menuEl) observer.observe(menuEl);
    reposition();
    menuEl?.focus({ preventScroll: true });
    window.addEventListener('pointerdown', dismissOutside, true);
    window.addEventListener('resize', reposition);
    window.addEventListener('scroll', reposition, true);
    return () => {
      observer.disconnect();
      window.removeEventListener('pointerdown', dismissOutside, true);
      window.removeEventListener('resize', reposition);
      window.removeEventListener('scroll', reposition, true);
    };
  });

  function run() {
    screen.notice = '';
    let captured: AiTarget | null;
    try {
      captured = captureTarget(screen.scope);
    } catch (err) {
      screen.notice = err instanceof Error ? err.message : String(err);
      return;
    }
    if (!captured) {
      screen.notice = screen.scope === 'selection' ? 'Select some text first.' : 'The note has no text to send.';
      return;
    }
    if (aiStore.panelOperation === 'translate' && !aiStore.targetLanguage.trim()) {
      screen.notice = 'Choose a language to translate into.';
      return;
    }
    screen.target = captured;
    void aiStore.start(aiStore.operation(), captured.text);
  }

  function apply(placement: AiPlacement) {
    if (!result || !screen.target) return;
    if (applyResult(screen.target, result.text, placement)) {
      onClose();
    } else {
      screen.notice = 'The note changed after the request started, so Jotter did not apply the result. Run the command again.';
    }
  }

  async function copyResult() {
    if (!result) return;
    const copiedScreen = screen;
    try {
      await copyToClipboard({ text: result.text, mode: 'markdown_source' });
      copiedScreen.notice = 'Copied the result.';
    } catch (err) {
      copiedScreen.notice = `Copy failed: ${err}`;
    }
  }

  function usageLabel() {
    if (!result) return '';
    const { inputTokens, outputTokens } = result.usage;
    const tokens = inputTokens !== null && outputTokens !== null ? ` · ${inputTokens} in, ${outputTokens} out tokens` : '';
    return `${result.model} via ${AI_PROVIDER_LABELS[result.provider]}${tokens}`;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      onClose();
      return;
    }
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

<div
  class="ai-panel"
  role="dialog"
  aria-label="AI"
  tabindex="-1"
  bind:this={menuEl}
  style:left="{position.left}px"
  style:top="{position.top}px"
  onkeydown={handleKeydown}
>
    <header class="ai-head">
      <button
        type="button"
        class="move-handle"
        class:dragging={drag !== null}
        aria-label="Move AI menu"
        title="Drag to move. Arrow keys move; Shift moves faster. Home resets position."
        onpointerdown={startDrag}
        onpointermove={moveDrag}
        onpointerup={() => (drag = null)}
        onpointercancel={() => (drag = null)}
        onlostpointercapture={() => (drag = null)}
        onkeydown={moveWithKeyboard}
      ><span aria-hidden="true">⠿</span> AI</button>
      <button type="button" class="close" aria-label="Close AI menu" onclick={onClose}>×</button>
    </header>
      <Segmented
        label="Command"
        fill
        options={OPERATIONS}
        value={aiStore.panelOperation}
        onchange={(kind) => (aiStore.panelOperation = kind)}
      />

    {#if !aiStore.status}
      <p class="muted">{aiStore.loadError || 'Loading AI settings…'}</p>
    {:else if !ready}
      <div class="setup">
        <p>
          {aiStore.enabled
            ? `Set a base URL and model for ${AI_PROVIDER_LABELS[provider]}.`
            : 'AI is off.'}
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
          value={screen.scope}
          onchange={(next) => (screen.scope = next)}
        />
        {#if aiStore.panelOperation === 'rewrite'}
          <label class="field">
            <span>Action</span>
            <select
              value={aiStore.rewriteStyle}
              onchange={(e) => aiStore.setRewriteStyle((e.currentTarget as HTMLSelectElement).value as RewriteStyle)}
            >
              {#each REWRITE_ACTIONS as value (value)}
                <option {value}>{REWRITE_STYLE_LABELS[value]}</option>
              {/each}
              <optgroup label="Tone">
                {#each REWRITE_TONES as value (value)}
                  <option {value}>{REWRITE_STYLE_LABELS[value]}</option>
                {/each}
              </optgroup>
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
          <button type="button" class="link" onclick={onOpenSettings}>Edit writing style</button>
        </p>
      {/if}

      <p class="muted">{endpoint?.model} · {destination}</p>

      <div class="result-area">
        {#if aiStore.run.phase === 'running'}
          <div class="status" role="status">Waiting for {AI_PROVIDER_LABELS[provider]}…</div>
        {:else if aiStore.run.phase === 'error'}
          <div class="error" role="alert">{aiStore.run.message}</div>
        {:else if result}
          <label class="muted" for="ai-result-preview">Markdown preview</label>
          <textarea
            id="ai-result-preview"
            bind:this={resultEl}
            class="result"
            readonly
            aria-label="Result preview"
            value={result.text}
          ></textarea>
          <div class="muted small">{usageLabel()}</div>
        {:else}
          <div class="status muted">The result appears here.</div>
        {/if}
      </div>
    {/if}

    {#if screen.notice}<div class="notice" role="status">{screen.notice}</div>{/if}

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

<style>
  .ai-panel {
    position: fixed;
    z-index: 190;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    box-sizing: border-box;
    width: min(440px, calc(100vw - 24px));
    max-height: calc(100dvh - 24px);
    overflow: auto;
    padding: var(--space-4);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-card);
    background: var(--surface-raised);
    box-shadow: var(--shadow-popover);
  }

  .ai-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .move-handle {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) 0;
    border: 0;
    background: transparent;
    color: var(--text-primary);
    text-align: left;
    font: 600 var(--fs-16) / 1.25 var(--font-ui);
    cursor: grab;
    touch-action: none;
    user-select: none;
  }

  .move-handle.dragging {
    cursor: grabbing;
  }

  .close {
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-secondary);
    font: inherit;
    font-size: 20px;
    cursor: pointer;
    padding: 0 var(--space-2);
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
    flex: 1;
    min-width: min(200px, 100%);
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
    flex: none;
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
    box-sizing: border-box;
    width: 100%;
    height: 160px;
    min-height: 100px;
    padding: var(--space-3);
    border: 1px solid var(--line-control);
    border-radius: var(--radius-card);
    background: var(--surface-editor);
    color: var(--text-primary);
    font: var(--fs-13) / 1.6 var(--font-mono);
    resize: vertical;
  }

  .notice {
    color: var(--text-secondary);
    font-size: var(--fs-12);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
